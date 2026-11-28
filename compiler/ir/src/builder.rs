use std::collections::{HashMap, HashSet};

use nex_ast::{Block, Expr, ExprKind, Program, Stmt, StmtKind};
use nex_semantic::{Analysis, Type};

use crate::types::{
    BinOp, BlockId, CastKind, Const, ExternDecl, Inst, IrFunction, IrModule, IrType, Operand,
    StructDef, Terminator, UnOp, ValueId,
};

#[derive(Clone, Copy)]
struct Slot {
    ptr: ValueId,
    ty: IrType,
}

struct FunctionBuilder {
    func: IrFunction,
    next_value: u32,
    next_block: u32,
    current: BlockId,
    entry_alloca_index: usize,
    loop_stack: Vec<(BlockId, BlockId)>,
    /// Slots (alloca pointers) of local growable arrays `[T]`.
    growable_slots: std::collections::HashSet<ValueId>,
    /// Subset of `growable_slots` that currently own their heap buffer and must
    /// be freed at scope exit (RAII). A slot leaves this set when it is assigned
    /// a borrowed value (e.g. from another variable or a parameter).
    owned: std::collections::HashSet<ValueId>,
    /// Local struct slots that own growable-array fields and must have those
    /// fields freed at scope exit (RAII). Only locals (not parameters) are
    /// registered; deep-copy on struct copy keeps them independent.
    owned_structs: Vec<(ValueId, &'static str)>,
}

impl FunctionBuilder {
    fn new(name: String, params: Vec<IrType>, ret: IrType) -> Self {
        let params = params
            .into_iter()
            .enumerate()
            .map(|(index, ty)| (ValueId(index as u32), ty))
            .collect();
        let func = IrFunction {
            name,
            params,
            ret,
            blocks: vec![crate::types::BasicBlock {
                id: BlockId(0),
                insts: Vec::new(),
                terminator: None,
            }],
        };
        let param_count = func.params.len() as u32;
        Self {
            func,
            next_value: param_count,
            next_block: 1,
            current: BlockId(0),
            entry_alloca_index: 0,
            loop_stack: Vec::new(),
            growable_slots: std::collections::HashSet::new(),
            owned: std::collections::HashSet::new(),
            owned_structs: Vec::new(),
        }
    }

    /// Emits `free(slot)` (load the header pointer, call `nex_rt_array_free`).
    /// `free(NULL)` is a safe no-op in the runtime, so uninitialised slots are
    /// handled as long as they were null/empty initialised.
    fn emit_array_free(&mut self, slot: ValueId) {
        let value = self.new_value();
        self.emit(Inst::Load {
            dst: value,
            ptr: slot,
            ty: IrType::String,
        });
        self.emit(Inst::Call {
            dst: None,
            name: "nex_rt_array_free".to_string(),
            args: vec![Operand::Value(value)],
            ret: IrType::Void,
        });
    }

    fn new_value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        self.func.blocks.push(crate::types::BasicBlock {
            id,
            insts: Vec::new(),
            terminator: None,
        });
        id
    }

    fn current_block(&mut self) -> &mut crate::types::BasicBlock {
        &mut self.func.blocks[self.current.0 as usize]
    }

    fn emit(&mut self, inst: Inst) {
        self.current_block().insts.push(inst);
    }

    fn terminate(&mut self, terminator: Terminator) {
        self.current_block().terminator = Some(terminator);
    }

    fn current_terminated(&self) -> bool {
        self.func.blocks[self.current.0 as usize]
            .terminator
            .is_some()
    }

    fn alloca_at_entry(&mut self, ty: IrType) -> ValueId {
        let dst = self.new_value();
        let inst = Inst::Alloca { dst, ty };
        self.func.blocks[0]
            .insts
            .insert(self.entry_alloca_index, inst);
        self.entry_alloca_index += 1;
        dst
    }
}

pub fn build(program: &Program, analysis: &Analysis) -> Result<IrModule, String> {
    let structs = analysis
        .structs
        .values()
        .filter(|info| {
            info.type_params.is_empty()
                && !info
                    .fields
                    .iter()
                    .any(|(_, ty)| matches!(ty, Type::TypeParam(_)))
        })
        .map(|info| StructDef {
            name: info.name_static,
            fields: info
                .fields
                .iter()
                .map(|(_, ty)| IrType::from_type(*ty))
                .collect(),
        })
        .collect();

    let mut module = IrModule {
        structs,
        functions: Vec::new(),
        externs: Vec::new(),
    };

    let mut functions = program.functions.clone();
    functions.extend(analysis.mono_functions.clone());
    // Skip generic (un-monomorphized) functions.
    functions.retain(|f| {
        if analysis.generic_functions.contains(&f.name) {
            return false;
        }
        analysis
            .functions
            .get(&f.name)
            .map(|s| s.type_params.is_empty())
            .unwrap_or(false)
    });
    for block in &program.impls {
        for method in &block.methods {
            let mut method = method.clone();
            method.name = format!("{}.{}", block.type_name, method.name);
            functions.push(method);
        }
    }

    for function in &functions {
        if analysis.generic_functions.contains(&function.name) {
            continue;
        }
        if !analysis.functions.contains_key(&function.name) {
            continue;
        }
        if analysis.externs.contains(&function.name) {
            let signature = analysis.functions.get(&function.name).ok_or_else(|| {
                format!("missing type information for function `{}`", function.name)
            })?;
            module.externs.push(ExternDecl {
                name: function.name.clone(),
                params: signature
                    .params
                    .iter()
                    .copied()
                    .map(IrType::from_type)
                    .collect(),
                ret: IrType::from_type(signature.ret),
            });
            continue;
        }

        let signature = analysis
            .functions
            .get(&function.name)
            .ok_or_else(|| format!("missing type information for function `{}`", function.name))?;

        let ret = IrType::from_type(signature.ret);
        let params: Vec<IrType> = signature
            .params
            .iter()
            .copied()
            .map(IrType::from_type)
            .collect();
        let mut builder = FunctionBuilder::new(function.name.clone(), params, ret);

        let mut scopes: Vec<HashMap<String, Slot>> = Vec::new();
        let mut param_scope = HashMap::new();
        let ir_params = builder.func.params.clone();
        for (index, (param, (value, ty))) in
            function.params.iter().zip(ir_params.iter()).enumerate()
        {
            // Parameters are stored in stack slots so that struct parameters
            // (and scalar parameters) are addressable and use value semantics.
            let ptr = builder.alloca_at_entry(*ty);
            let operand = Operand::Value(*value);
            if let Some(Type::Struct(name)) = signature.params.get(index).copied() {
                // Value semantics: the callee owns its own copy of every
                // growable-array field, so mutating `param.field` cannot change
                // the caller's struct. The caller keeps its buffers.
                let incoming = builder.alloca_at_entry(*ty);
                builder.emit(Inst::Store {
                    ptr: incoming,
                    value: operand,
                    ty: *ty,
                });
                copy_struct(&mut builder, ptr, incoming, name, analysis)?;
                builder.owned_structs.push((ptr, name));
            } else {
                builder.emit(Inst::Store {
                    ptr,
                    value: operand,
                    ty: *ty,
                });
            }
            param_scope.insert(param.name.clone(), Slot { ptr, ty: *ty });
        }
        scopes.push(param_scope);

        let mut locals = Vec::new();
        collect_locals(&function.body, analysis, &mut locals);
        let mut storage: HashMap<nex_diagnostics::Span, (ValueId, IrType)> = HashMap::new();
        let mut seen_locals: HashSet<nex_diagnostics::Span> = HashSet::new();
        for (span, ty) in locals {
            if !seen_locals.insert(span) {
                continue;
            }
            let ir_ty = IrType::from_type(ty);
            let ptr = builder.alloca_at_entry(ir_ty);
            // Growable arrays own a heap header. Initialise the slot to an empty
            // array so a scope exit before the declaration frees a valid pointer,
            // and treat it as owned until reassigned from a borrowed value.
            if matches!(ty, Type::DynArray(_)) {
                let empty = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(empty),
                    name: "nex_rt_array_new".to_string(),
                    args: Vec::new(),
                    ret: IrType::String,
                });
                builder.emit(Inst::Store {
                    ptr,
                    value: Operand::Value(empty),
                    ty: IrType::String,
                });
                builder.growable_slots.insert(ptr);
                builder.owned.insert(ptr);
            }
            if let Type::Struct(name) = ty {
                builder.owned_structs.push((ptr, name));
            }
            storage.insert(span, (ptr, ir_ty));
        }

        let mut deferred: Vec<(ValueId, Block)> = Vec::new();
        scopes.push(HashMap::new());
        lower_stmts(
            &mut builder,
            &function.body.stmts,
            &mut scopes,
            &storage,
            analysis,
            &mut deferred,
        )?;
        scopes.pop();

        if !builder.current_terminated() {
            lower_deferred(&mut builder, &deferred, &mut scopes, &storage, analysis)?;
            if ret == IrType::Void {
                builder.terminate(Terminator::Ret(None));
            } else {
                return Err(format!(
                    "function `{}` may exit without returning a value",
                    function.name
                ));
            }
        }

        // RAII: before every `ret`, free each growable-array local that still
        // owns its buffer. `insts` are emitted before the terminator, so pushing
        // frees onto a Ret block runs them just before returning. Each block has
        // at most one executed Ret per run, so every owned slot is freed once.
        if !builder.owned.is_empty() {
            let ret_blocks: Vec<usize> = builder
                .func
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| matches!(block.terminator, Some(Terminator::Ret(_))))
                .map(|(index, _)| index)
                .collect();
            let owned_slots: Vec<ValueId> = builder.owned.iter().copied().collect();
            for block_index in ret_blocks {
                let mut frees = Vec::new();
                for slot in &owned_slots {
                    let loaded = builder.new_value();
                    frees.push(Inst::Load {
                        dst: loaded,
                        ptr: *slot,
                        ty: IrType::String,
                    });
                    frees.push(Inst::Call {
                        dst: None,
                        name: "nex_rt_array_free".to_string(),
                        args: vec![Operand::Value(loaded)],
                        ret: IrType::Void,
                    });
                }
                builder.func.blocks[block_index].insts.extend(frees);
            }
        }

        // RAII for structs: free growable-array fields of owned struct locals
        // before returning.
        if !builder.owned_structs.is_empty() {
            let ret_blocks: Vec<usize> = builder
                .func
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| matches!(block.terminator, Some(Terminator::Ret(_))))
                .map(|(index, _)| index)
                .collect();
            let owned_structs: Vec<(ValueId, &'static str)> = builder.owned_structs.clone();
            let saved_current = builder.current;
            for block_index in ret_blocks {
                builder.current = BlockId(block_index as u32);
                for (slot, struct_name) in &owned_structs {
                    free_struct_arrays(&mut builder, *slot, struct_name, analysis);
                }
            }
            builder.current = saved_current;
        }

        module.functions.push(builder.func);
    }

    Ok(module)
}

fn collect_locals(
    block: &Block,
    analysis: &Analysis,
    out: &mut Vec<(nex_diagnostics::Span, Type)>,
) {
    for statement in &block.stmts {
        match &statement.kind {
            StmtKind::Let { .. } => {
                if let Some(ty) = analysis.decl_type(statement.span) {
                    out.push((statement.span, ty));
                }
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (_, body) in branches {
                    collect_locals(body, analysis, out);
                }
                if let Some(body) = else_branch {
                    collect_locals(body, analysis, out);
                }
            }
            StmtKind::While { body, .. } => {
                collect_locals(body, analysis, out);
            }
            StmtKind::For { body, .. } => {
                collect_locals(body, analysis, out);
            }
            StmtKind::Match { arms, .. } => {
                for arm in arms {
                    collect_locals(&arm.body, analysis, out);
                }
            }
            StmtKind::Unsafe(block) => {
                collect_locals(block, analysis, out);
            }
            StmtKind::Defer(block) => {
                collect_locals(block, analysis, out);
            }
            _ => {}
        }
    }
}

fn lower_stmts(
    builder: &mut FunctionBuilder,
    statements: &[Stmt],
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    for statement in statements {
        if builder.current_terminated() {
            break;
        }
        lower_statement(builder, statement, scopes, storage, analysis, deferred)?;
    }
    if !builder.current_terminated() {
        lower_deferred(builder, deferred, scopes, storage, analysis)?;
    }
    Ok(())
}

fn lower_statement(
    builder: &mut FunctionBuilder,
    statement: &Stmt,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    match &statement.kind {
        StmtKind::Let {
            names: Some(names),
            init,
            ..
        } => {
            let ty = analysis
                .expr_type(init.span)
                .ok_or_else(|| "missing type information for destructuring".to_string())?;
            let Type::Struct(struct_name) = ty else {
                return Err("cannot destructure a non-tuple value".to_string());
            };
            let info = analysis
                .struct_info(struct_name)
                .ok_or_else(|| "missing struct information for destructuring".to_string())?;
            let source = lower_struct_place(builder, init, scopes, storage, analysis)?;
            for (index, binding) in names.iter().enumerate() {
                let field_ty = info
                    .fields
                    .get(index)
                    .map(|(_, ty)| *ty)
                    .unwrap_or(Type::Error);
                let ir_ty = IrType::from_type(field_ty);
                let field_ptr = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: field_ptr,
                    base: source,
                    struct_name,
                    index: index as u32,
                    field_ty: ir_ty,
                });
                let value = builder.new_value();
                builder.emit(Inst::Load {
                    dst: value,
                    ptr: field_ptr,
                    ty: ir_ty,
                });
                let slot = builder.alloca_at_entry(ir_ty);
                builder.emit(Inst::Store {
                    ptr: slot,
                    value: Operand::Value(value),
                    ty: ir_ty,
                });
                if let Some(scope) = scopes.last_mut() {
                    scope.insert(
                        binding.clone(),
                        Slot {
                            ptr: slot,
                            ty: ir_ty,
                        },
                    );
                }
            }
            Ok(())
        }
        StmtKind::Let { name, init, .. } => {
            let (ptr, ty) = storage
                .get(&statement.span)
                .copied()
                .ok_or_else(|| format!("missing storage for binding `{name}`"))?;
            let expected = analysis.decl_type(statement.span).unwrap_or(Type::Error);

            if let Type::Struct(struct_name) = expected {
                let source = lower_struct_place(builder, init, scopes, storage, analysis)?;
                copy_struct(builder, ptr, source, struct_name, analysis)?;
            } else if let Type::Array { display, .. } = expected {
                let source = lower_array_place(builder, init, scopes, storage, analysis)?;
                let info = analysis
                    .array_info(display)
                    .ok_or_else(|| "missing array type information".to_string())?;
                copy_array(
                    builder, ptr, source, info.llvm, info.elem, info.len, analysis,
                )?;
            } else {
                if builder.growable_slots.contains(&ptr) && builder.owned.contains(&ptr) {
                    builder.emit_array_free(ptr);
                }
                let value = lower_expr(builder, init, scopes, storage, analysis, expected)?;
                builder.emit(Inst::Store { ptr, value, ty });
                if matches!(expected, Type::DynArray(_)) {
                    if matches!(init.kind, ExprKind::Ident(_)) {
                        // Bound from another variable: this slot borrows, not owns.
                        builder.owned.remove(&ptr);
                    } else {
                        builder.owned.insert(ptr);
                    }
                }
            }
            if let Some(scope) = scopes.last_mut() {
                scope.insert(name.clone(), Slot { ptr, ty });
            }
            Ok(())
        }
        StmtKind::AssignMany { names, values } => {
            let mut temps: Vec<(ValueId, IrType)> = Vec::new();
            for (name, value) in names.iter().zip(values.iter()) {
                let ty = lookup_slot(scopes, name)
                    .map(|slot| slot.ty)
                    .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
                let ptr = builder.alloca_at_entry(ty);
                match ty {
                    IrType::Struct(struct_name) => {
                        // Parallel assignment must read every source before any
                        // target is overwritten, so the first pass deep-clones
                        // each value into an independent temporary.
                        let source = lower_struct_place(builder, value, scopes, storage, analysis)?;
                        copy_struct(builder, ptr, source, struct_name, analysis)?;
                    }
                    IrType::Array(array_type) => {
                        let source = lower_array_place(builder, value, scopes, storage, analysis)?;
                        let src_ty = analysis.expr_type(value.span).unwrap_or(Type::Error);
                        let Type::Array { display, .. } = src_ty else {
                            return Err("array assignment requires an array value".to_string());
                        };
                        let info = analysis
                            .array_info(display)
                            .ok_or_else(|| "missing array type information".to_string())?;
                        copy_array(
                            builder, ptr, source, array_type, info.elem, info.len, analysis,
                        )?;
                    }
                    _ => {
                        let operand =
                            lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                        builder.emit(Inst::Store {
                            ptr,
                            value: operand,
                            ty,
                        });
                    }
                }
                temps.push((ptr, ty));
            }
            for (i, (name, (ptr, ty))) in names.iter().zip(temps.iter()).enumerate() {
                let source_expr = values.get(i);
                let target = lookup_slot(scopes, name)
                    .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?
                    .ptr;
                match ty {
                    IrType::Struct(struct_name) => {
                        // The temporary owns its own clones, so freeing the
                        // target's old buffers first is safe.
                        if builder.owned_structs.iter().any(|(s, _)| *s == target) {
                            free_struct_arrays(builder, target, struct_name, analysis);
                        }
                        copy_struct(builder, target, *ptr, struct_name, analysis)?;
                        // The temporary is never used again; release its clones.
                        free_struct_arrays(builder, *ptr, struct_name, analysis);
                    }
                    IrType::Array(array_type) => {
                        let src_ty = analysis
                            .expr_type(values.get(i).map(|v| v.span).unwrap_or_default())
                            .unwrap_or(Type::Error);
                        if let Type::Array { display, .. } = src_ty
                            && let Some(info) = analysis.array_info(display)
                        {
                            copy_array(
                                builder, target, *ptr, array_type, info.elem, info.len, analysis,
                            )?;
                        }
                    }
                    _ => {
                        if builder.growable_slots.contains(&target)
                            && builder.owned.contains(&target)
                        {
                            builder.emit_array_free(target);
                        }
                        let value = builder.new_value();
                        builder.emit(Inst::Load {
                            dst: value,
                            ptr: *ptr,
                            ty: *ty,
                        });
                        builder.emit(Inst::Store {
                            ptr: target,
                            value: Operand::Value(value),
                            ty: *ty,
                        });
                        if builder.growable_slots.contains(&target) {
                            let borrow = source_expr
                                .map(|v| matches!(&v.kind, ExprKind::Ident(_)))
                                .unwrap_or(false);
                            if borrow {
                                builder.owned.remove(&target);
                            } else {
                                builder.owned.insert(target);
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        StmtKind::Assign { name, value } => {
            if let Some(global) = analysis.globals.get(name) {
                let new_value = lower_expr(builder, value, scopes, storage, analysis, global.ty)?;
                lower_global_set(builder, global.slot, global.ty, new_value);
                return Ok(());
            }
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
            match (slot.ptr, slot.ty) {
                (ptr, IrType::Struct(struct_name)) => {
                    let source = lower_struct_place(builder, value, scopes, storage, analysis)?;
                    // Self-assignment (`s = s`) must not free the fields it is
                    // about to copy from.
                    if source != ptr {
                        // Free the previous struct's growable-array fields before
                        // overwriting, mirroring scalar/growable reassignment.
                        if builder.owned_structs.iter().any(|(s, _)| *s == ptr) {
                            free_struct_arrays(builder, ptr, struct_name, analysis);
                        }
                        copy_struct(builder, ptr, source, struct_name, analysis)?;
                    }
                    Ok(())
                }
                (ptr, IrType::Array(array_type)) => {
                    let source = lower_array_place(builder, value, scopes, storage, analysis)?;
                    let src_ty = analysis.expr_type(value.span).unwrap_or(Type::Error);
                    let (elem, len) = match src_ty {
                        Type::Array { display, .. } => {
                            let info = analysis
                                .array_info(display)
                                .ok_or_else(|| "missing array type information".to_string())?;
                            (info.elem, info.len)
                        }
                        _ => return Err("array assignment requires an array value".to_string()),
                    };
                    copy_array(builder, ptr, source, array_type, elem, len, analysis)?;
                    Ok(())
                }
                (ptr, ty) => {
                    // Self-assignment (`x = x`) must not free the slot's buffer
                    // before reading it back: that would be a use-after-free.
                    if builder.growable_slots.contains(&ptr)
                        && builder.owned.contains(&ptr)
                        && !matches!(&value.kind, ExprKind::Ident(ident) if ident == name)
                    {
                        builder.emit_array_free(ptr);
                    }
                    let operand =
                        lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                    builder.emit(Inst::Store {
                        ptr,
                        value: operand,
                        ty,
                    });
                    if builder.growable_slots.contains(&ptr) {
                        if matches!(&value.kind, ExprKind::Ident(ident) if ident == name) {
                            // Keep ownership: the slot still owns its (possibly
                            // unchanged) buffer.
                            builder.owned.insert(ptr);
                        } else if matches!(value.kind, ExprKind::Ident(_)) {
                            builder.owned.remove(&ptr);
                        } else {
                            builder.owned.insert(ptr);
                        }
                    }
                    Ok(())
                }
            }
        }
        StmtKind::AssignField {
            base,
            path: _,
            value,
        } => {
            let Some(steps) = analysis.field_write(statement.span) else {
                return Err(format!(
                    "missing field information for assignment to `{base}`"
                ));
            };
            let slot = lookup_slot(scopes, base)
                .ok_or_else(|| format!("unknown variable `{base}` during code generation"))?;
            let mut ptr = slot.ptr;
            let mut last_ty = IrType::Void;
            let mut last_field_ty = Type::Error;
            for (struct_name, index, ty) in &steps {
                last_ty = IrType::from_type(*ty);
                last_field_ty = *ty;
                let next = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: next,
                    base: ptr,
                    struct_name,
                    index: *index,
                    field_ty: last_ty,
                });
                ptr = next;
            }
            // Assigning to a field follows the same value semantics as a whole
            // variable: a struct value is deep-cloned into the field, and a
            // growable array is deep-cloned too, so the source stays valid and
            // cannot be freed while the field still points at it.
            if let Type::Struct(name) = last_field_ty {
                let source = lower_struct_place(builder, value, scopes, storage, analysis)?;
                if source != ptr {
                    free_struct_arrays(builder, ptr, name, analysis);
                    copy_struct(builder, ptr, source, name, analysis)?;
                }
                return Ok(());
            }
            if let Type::DynArray(display) = last_field_ty {
                let info = analysis.array_info(display).cloned();
                let elem = info.as_ref().map(|i| i.elem).unwrap_or(Type::Error);
                let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                let cloned = if let Type::Struct(elem_struct) = elem {
                    let operand_id = operand_value_id(builder, operand)?;
                    clone_struct_array(builder, operand_id, elem_struct, analysis)?
                } else {
                    let elem_size = info
                        .map(|i| ir_elem_size(analysis, IrType::from_type(i.elem)))
                        .unwrap_or(8);
                    let c = builder.new_value();
                    builder.emit(Inst::Call {
                        dst: Some(c),
                        name: "nex_rt_array_clone".to_string(),
                        args: vec![operand, Operand::Const(Const::Int(elem_size))],
                        ret: IrType::String,
                    });
                    c
                };
                let old_header = builder.new_value();
                builder.emit(Inst::Load {
                    dst: old_header,
                    ptr,
                    ty: IrType::String,
                });
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_array_free".to_string(),
                    args: vec![Operand::Value(old_header)],
                    ret: IrType::Void,
                });
                builder.emit(Inst::Store {
                    ptr,
                    value: Operand::Value(cloned),
                    ty: IrType::String,
                });
                return Ok(());
            }
            let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
            builder.emit(Inst::Store {
                ptr,
                value: operand,
                ty: last_ty,
            });
            Ok(())
        }
        StmtKind::AssignFieldDeref {
            reference,
            path: _,
            value,
        } => {
            let Some(steps) = analysis.field_write(statement.span) else {
                return Err(
                    "missing field information for assignment through a reference".to_string(),
                );
            };
            let base = lower_expr(builder, reference, scopes, storage, analysis, Type::Error)?;
            let mut ptr = operand_value_id(builder, base)?;
            let mut last_ty = IrType::Void;
            for (struct_name, index, ty) in &steps {
                last_ty = IrType::from_type(*ty);
                let next = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: next,
                    base: ptr,
                    struct_name,
                    index: *index,
                    field_ty: last_ty,
                });
                ptr = next;
            }
            let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
            builder.emit(Inst::Store {
                ptr,
                value: operand,
                ty: last_ty,
            });
            Ok(())
        }
        StmtKind::If {
            branches,
            else_branch,
        } => lower_if(
            builder,
            branches,
            else_branch,
            scopes,
            storage,
            analysis,
            deferred,
        ),
        StmtKind::While { cond, body } => {
            lower_while(builder, cond, body, scopes, storage, analysis, deferred)
        }
        StmtKind::For {
            var,
            start,
            end,
            body,
        } => lower_for(
            builder,
            var,
            (start, end),
            body,
            scopes,
            storage,
            analysis,
            deferred,
        ),
        StmtKind::Match { scrutinee, arms } => lower_match(
            builder, scrutinee, arms, scopes, storage, analysis, deferred,
        ),
        StmtKind::AssignIndex { base, index, value } => {
            let slot = lookup_slot(scopes, base)
                .ok_or_else(|| format!("unknown variable `{base}` during code generation"))?;
            // A growable array is stored as a header pointer, so the slot type is
            // a plain pointer and the element type comes from the assigned value.
            if slot.ty == IrType::String {
                let elem_ty = analysis.expr_type(value.span).unwrap_or(Type::Error);
                let header = builder.new_value();
                builder.emit(Inst::Load {
                    dst: header,
                    ptr: slot.ptr,
                    ty: IrType::String,
                });
                return lower_dyn_assign_index(
                    builder, header, index, value, elem_ty, scopes, storage, analysis,
                );
            }
            let Slot {
                ptr: base_ptr,
                ty: IrType::Array(array_type),
            } = slot
            else {
                return Err(format!("cannot assign to an element of `{base}` by value"));
            };
            let idx_value = lower_expr(builder, index, scopes, storage, analysis, Type::Error)?;
            let index_ty = analysis.expr_type(index.span).unwrap_or(Type::I64);
            let info = analysis
                .arrays
                .values()
                .find(|candidate| candidate.llvm == array_type)
                .cloned();
            if let Some(found) = &info {
                lower_bounds_check(
                    builder,
                    idx_value.clone(),
                    index_ty,
                    Operand::Const(Const::Int(found.len as i64)),
                );
            }
            let elem = info.map(|found| found.elem).unwrap_or(Type::Error);
            let elem_ir = IrType::from_type(elem);
            let dst_ptr = builder.new_value();
            builder.emit(Inst::IndexPtr {
                dst: dst_ptr,
                base: base_ptr,
                array_type,
                index: idx_value,
                elem_ty: elem_ir,
            });
            if let Type::Struct(struct_name) = elem {
                let source = lower_struct_place(builder, value, scopes, storage, analysis)?;
                copy_struct(builder, dst_ptr, source, struct_name, analysis)?;
                return Ok(());
            }
            let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
            builder.emit(Inst::Store {
                ptr: dst_ptr,
                value: operand,
                ty: elem_ir,
            });
            Ok(())
        }
        StmtKind::AssignDeref { place, value } => {
            let ptr = lower_expr(builder, place, scopes, storage, analysis, Type::Error)?;
            let ptr_id = operand_value_id(builder, ptr)?;
            let value_ty = analysis.expr_type(value.span).unwrap_or(Type::Error);
            let ir_ty = IrType::from_type(value_ty);
            let operand = lower_expr(builder, value, scopes, storage, analysis, value_ty)?;
            builder.emit(Inst::Store {
                ptr: ptr_id,
                value: operand,
                ty: ir_ty,
            });
            Ok(())
        }
        StmtKind::AssignIndexDeref {
            reference,
            index,
            value,
        } => {
            let pointee = match analysis.expr_type(reference.span) {
                Some(Type::Ref { inner, .. }) => resolve_named_type(analysis, inner),
                _ => Type::Error,
            };
            let elem_ty = analysis.expr_type(value.span).unwrap_or(Type::Error);
            let base = lower_expr(builder, reference, scopes, storage, analysis, Type::Error)?;
            let base_id = operand_value_id(builder, base)?;
            match pointee {
                Type::DynArray(_) => {
                    // `reference` points at a slot holding the growable header
                    // pointer; load it, then assign through the header.
                    let header = builder.new_value();
                    builder.emit(Inst::Load {
                        dst: header,
                        ptr: base_id,
                        ty: IrType::String,
                    });
                    lower_dyn_assign_index(
                        builder, header, index, value, elem_ty, scopes, storage, analysis,
                    )
                }
                Type::Array { .. } => {
                    // `reference` points at the inline array; index it directly.
                    let idx_value =
                        lower_expr(builder, index, scopes, storage, analysis, Type::Error)?;
                    let index_ty = analysis.expr_type(index.span).unwrap_or(Type::I64);
                    let array_info = analysis
                        .arrays
                        .values()
                        .find(|candidate| {
                            matches!(pointee, Type::Array { llvm, .. } if llvm == candidate.llvm)
                        })
                        .cloned();
                    if let Some(found) = &array_info {
                        lower_bounds_check(
                            builder,
                            idx_value.clone(),
                            index_ty,
                            Operand::Const(Const::Int(found.len as i64)),
                        );
                    }
                    let elem = array_info.map(|found| found.elem).unwrap_or(Type::Error);
                    let elem_ir = IrType::from_type(elem);
                    let dst_ptr = builder.new_value();
                    let array_llvm = match pointee {
                        Type::Array { llvm, .. } => llvm,
                        _ => unreachable!(),
                    };
                    builder.emit(Inst::IndexPtr {
                        dst: dst_ptr,
                        base: base_id,
                        array_type: array_llvm,
                        index: idx_value,
                        elem_ty: elem_ir,
                    });
                    let operand =
                        lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                    builder.emit(Inst::Store {
                        ptr: dst_ptr,
                        value: operand,
                        ty: elem_ir,
                    });
                    Ok(())
                }
                _ => {
                    Err("cannot index-assign through a reference to a non-array value".to_string())
                }
            }
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                // Returning an owned growable array transfers ownership to the
                // caller: do not free it at scope exit.
                if let ExprKind::Ident(name) = &value.kind {
                    if let Some(slot) = lookup_slot(scopes, name) {
                        if builder.growable_slots.contains(&slot.ptr) {
                            builder.owned.remove(&slot.ptr);
                        }
                        builder.owned_structs.retain(|(s, _)| *s != slot.ptr);
                    }
                }
                let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                lower_deferred(builder, deferred, scopes, storage, analysis)?;
                builder.terminate(Terminator::Ret(Some(operand)));
            } else {
                lower_deferred(builder, deferred, scopes, storage, analysis)?;
                builder.terminate(Terminator::Ret(None));
            }
            Ok(())
        }
        StmtKind::Break => {
            let (exit, _) = builder
                .loop_stack
                .last()
                .copied()
                .ok_or_else(|| "break outside of loop".to_string())?;
            builder.terminate(Terminator::Br(exit));
            Ok(())
        }
        StmtKind::Pass => Ok(()),
        StmtKind::Unsafe(block) => {
            lower_stmts(builder, &block.stmts, scopes, storage, analysis, deferred)
        }
        StmtKind::Defer(block) => {
            let flag = builder.alloca_at_entry(IrType::Bool);
            builder.emit(Inst::Store {
                ptr: flag,
                value: Operand::Const(Const::Bool(false)),
                ty: IrType::Bool,
            });
            deferred.push((flag, block.clone()));
            builder.emit(Inst::Store {
                ptr: flag,
                value: Operand::Const(Const::Bool(true)),
                ty: IrType::Bool,
            });
            Ok(())
        }
        StmtKind::Continue => {
            let (_, continue_bb) = builder
                .loop_stack
                .last()
                .copied()
                .ok_or_else(|| "continue outside of loop".to_string())?;
            builder.terminate(Terminator::Br(continue_bb));
            Ok(())
        }
        StmtKind::Expr(expr) => {
            let _ = lower_expr(builder, expr, scopes, storage, analysis, Type::Error)?;
            Ok(())
        }
    }
}

fn lower_if(
    builder: &mut FunctionBuilder,
    branches: &[(Expr, Block)],
    else_branch: &Option<Block>,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    let merge = builder.new_block();
    let mut merge_reachable = else_branch.is_none();
    let mut const_true_single_return = false;

    for (cond, body) in branches {
        let body_bb = builder.new_block();
        let next_bb = builder.new_block();
        let cond_is_true = matches!(cond.kind, ExprKind::Bool(true));
        let cond_value = lower_expr(builder, cond, scopes, storage, analysis, Type::Error)?;
        builder.terminate(Terminator::CondBr {
            cond: cond_value,
            then_bb: body_bb,
            else_bb: next_bb,
        });

        builder.current = body_bb;
        scopes.push(HashMap::new());
        lower_stmts(builder, &body.stmts, scopes, storage, analysis, deferred)?;
        scopes.pop();
        if !builder.current_terminated() {
            merge_reachable = true;
            builder.terminate(Terminator::Br(merge));
        } else if cond_is_true && else_branch.is_none() && branches.len() == 1 {
            const_true_single_return = true;
            merge_reachable = false;
        }

        builder.current = next_bb;
    }

    if let Some(body) = else_branch {
        scopes.push(HashMap::new());
        lower_stmts(builder, &body.stmts, scopes, storage, analysis, deferred)?;
        scopes.pop();
        if !builder.current_terminated() {
            merge_reachable = true;
            builder.terminate(Terminator::Br(merge));
        }
    } else if const_true_single_return {
        builder.terminate(Terminator::Unreachable);
    } else if !builder.current_terminated() {
        merge_reachable = true;
        builder.terminate(Terminator::Br(merge));
    }

    builder.current = merge;
    if !merge_reachable {
        builder.terminate(Terminator::Unreachable);
    }
    Ok(())
}

fn lower_while(
    builder: &mut FunctionBuilder,
    cond: &Expr,
    body: &Block,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    let cond_bb = builder.new_block();
    let body_bb = builder.new_block();
    let exit_bb = builder.new_block();

    builder.terminate(Terminator::Br(cond_bb));
    builder.current = cond_bb;
    let cond_value = lower_expr(builder, cond, scopes, storage, analysis, Type::Error)?;
    builder.terminate(Terminator::CondBr {
        cond: cond_value,
        then_bb: body_bb,
        else_bb: exit_bb,
    });

    builder.current = body_bb;
    builder.loop_stack.push((exit_bb, cond_bb));
    scopes.push(HashMap::new());
    lower_stmts(builder, &body.stmts, scopes, storage, analysis, deferred)?;
    scopes.pop();
    builder.loop_stack.pop();
    if !builder.current_terminated() {
        builder.terminate(Terminator::Br(cond_bb));
    }

    builder.current = exit_bb;
    if matches!(cond.kind, ExprKind::Bool(true)) {
        builder.terminate(Terminator::Unreachable);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn lower_for(
    builder: &mut FunctionBuilder,
    var: &str,
    range: (&Expr, &Expr),
    body: &Block,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    let (start, end) = range;
    let start_val = lower_expr(builder, start, scopes, storage, analysis, Type::Error)?;
    let end_val = lower_expr(builder, end, scopes, storage, analysis, Type::Error)?;
    let var_ty = analysis.expr_type(start.span).unwrap_or(Type::Error);

    // Allocate loop variable in entry and initialize.
    let var_ptr = builder.alloca_at_entry(IrType::from_type(var_ty));
    builder.emit(Inst::Store {
        ptr: var_ptr,
        value: start_val,
        ty: IrType::from_type(var_ty),
    });

    let cond_bb = builder.new_block();
    let body_bb = builder.new_block();
    let inc_bb = builder.new_block();
    let exit_bb = builder.new_block();

    // Condition: var < end
    builder.terminate(Terminator::Br(cond_bb));
    builder.current = cond_bb;
    let var_load = {
        let dst = builder.new_value();
        builder.emit(Inst::Load {
            dst,
            ptr: var_ptr,
            ty: IrType::from_type(var_ty),
        });
        dst
    };
    let cmp_dst = builder.new_value();
    builder.emit(Inst::Bin {
        dst: cmp_dst,
        op: BinOp::Lt,
        ty: IrType::from_type(var_ty),
        lhs: Operand::Value(var_load),
        rhs: end_val,
    });
    builder.terminate(Terminator::CondBr {
        cond: Operand::Value(cmp_dst),
        then_bb: body_bb,
        else_bb: exit_bb,
    });

    builder.current = body_bb;
    builder.loop_stack.push((exit_bb, inc_bb));
    scopes.push(HashMap::new());
    // expose var to body
    if let Some(scope) = scopes.last_mut() {
        scope.insert(
            var.to_string(),
            Slot {
                ptr: var_ptr,
                ty: IrType::from_type(var_ty),
            },
        );
    }
    lower_stmts(builder, &body.stmts, scopes, storage, analysis, deferred)?;
    scopes.pop();
    builder.loop_stack.pop();

    if !builder.current_terminated() {
        builder.terminate(Terminator::Br(inc_bb));
    }

    // increment
    builder.current = inc_bb;
    let var_load = {
        let dst = builder.new_value();
        builder.emit(Inst::Load {
            dst,
            ptr: var_ptr,
            ty: IrType::from_type(var_ty),
        });
        dst
    };
    let one = Operand::Const(Const::Int(1));
    let inc_dst = builder.new_value();
    builder.emit(Inst::Bin {
        dst: inc_dst,
        op: BinOp::Add,
        ty: IrType::from_type(var_ty),
        lhs: Operand::Value(var_load),
        rhs: one,
    });
    builder.emit(Inst::Store {
        ptr: var_ptr,
        value: Operand::Value(inc_dst),
        ty: IrType::from_type(var_ty),
    });
    builder.terminate(Terminator::Br(cond_bb));

    builder.current = exit_bb;
    Ok(())
}

fn lower_struct_place(
    builder: &mut FunctionBuilder,
    expr: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<ValueId, String> {
    if let ExprKind::Field { base, name, .. } = &expr.kind
        && let ExprKind::Ident(module_name) = &base.kind
        && analysis.modules.contains(module_name)
    {
        let qualified = format!("{}.{}", module_name, name);
        if let Some(init) = analysis.const_exprs.get(&qualified) {
            return lower_struct_place(builder, init, scopes, storage, analysis);
        }
    }
    match &expr.kind {
        ExprKind::Ident(name) => {
            let resolved = analysis
                .aliases
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone());
            if let Some(init) = analysis.const_exprs.get(&resolved) {
                return lower_struct_place(builder, init, scopes, storage, analysis);
            }
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;

            if matches!(slot.ty, IrType::Struct(_)) {
                Ok(slot.ptr)
            } else {
                Err(format!("variable `{name}` does not have a struct type"))
            }
        }
        ExprKind::Deref(inner) => {
            // `(*r)` as a place: the address the reference `r` holds.
            let op = lower_expr(builder, inner, scopes, storage, analysis, Type::Error)?;
            operand_value_id(builder, op)
        }
        ExprKind::Call { callee, args, .. } if callee == "pop" => {
            let ty = analysis
                .expr_type(expr.span)
                .ok_or_else(|| "missing type information for `pop`".to_string())?;
            let Type::Struct(name) = ty else {
                return Err("`pop` does not return a struct value".to_string());
            };
            let value = lower_expr(builder, expr, scopes, storage, analysis, ty)?;
            let ptr = builder.alloca_at_entry(IrType::Struct(name));
            builder.emit(Inst::Store {
                ptr,
                value,
                ty: IrType::Struct(name),
            });
            Ok(ptr)
        }
        ExprKind::Call { callee, args, .. } => {
            if analysis.method_call(expr.span).is_some() {
                let ty = analysis
                    .expr_type(expr.span)
                    .ok_or_else(|| "missing type information for method call".to_string())?;
                let Type::Struct(name) = ty else {
                    return Err("not a struct value".to_string());
                };
                let value = lower_expr(builder, expr, scopes, storage, analysis, ty)?;
                let ptr = builder.alloca_at_entry(IrType::Struct(name));
                builder.emit(Inst::Store {
                    ptr,
                    value,
                    ty: IrType::Struct(name),
                });
                return Ok(ptr);
            }

            let callee = if let Some(mono_name) = analysis.mono_struct(expr.span) {
                mono_name.as_str()
            } else {
                callee
            };
            // `clone(s)` and other struct-valued expressions are not places:
            // materialise the value in a temporary slot and use that as the
            // place. Binding through `copy_struct` then deep-clones arrays.
            if callee == "clone" {
                if let Some(Type::Struct(name)) = analysis.expr_type(expr.span) {
                    let value =
                        lower_expr(builder, expr, scopes, storage, analysis, Type::Struct(name))?;
                    let ptr = builder.alloca_at_entry(IrType::Struct(name));
                    builder.emit(Inst::Store {
                        ptr,
                        value,
                        ty: IrType::Struct(name),
                    });
                    return Ok(ptr);
                }
            }
            if let Some(info) = analysis.struct_info(callee) {
                let ptr = builder.alloca_at_entry(IrType::Struct(info.name_static));
                for (index, arg) in args.iter().enumerate() {
                    let Some((_, field_ty)) = info.fields.get(index) else {
                        return Err(format!("too many values for struct `{callee}`"));
                    };
                    let ir_ty = IrType::from_type(*field_ty);
                    let value = lower_expr(builder, arg, scopes, storage, analysis, *field_ty)?;
                    // Storing an owned growable array in a struct field moves
                    // the buffer into the struct. If the original local keeps
                    // ownership it is freed at scope exit and the field
                    // dangles: getting a struct back from a function used to
                    // read heap garbage because of exactly this.
                    if matches!(*field_ty, Type::DynArray(_)) {
                        if let ExprKind::Ident(name) = &arg.kind {
                            if let Some(slot) = lookup_slot(scopes, name) {
                                if builder.growable_slots.contains(&slot.ptr) {
                                    builder.owned.remove(&slot.ptr);
                                }
                            }
                        }
                    }
                    let field_ptr = builder.new_value();
                    builder.emit(Inst::FieldPtr {
                        dst: field_ptr,
                        base: ptr,
                        struct_name: info.name_static,
                        index: index as u32,
                        field_ty: ir_ty,
                    });
                    builder.emit(Inst::Store {
                        ptr: field_ptr,
                        value,
                        ty: ir_ty,
                    });
                }
                return Ok(ptr);
            }

            let signature = analysis
                .mono_call(expr.span)
                .and_then(|mono_name| analysis.function(mono_name))
                .or_else(|| analysis.function(callee))
                .ok_or_else(|| format!("unknown function `{callee}` during code generation"))?;
            let Type::Struct(name) = signature.ret else {
                return Err(format!("`{callee}` does not return a struct value"));
            };
            let value = lower_expr(builder, expr, scopes, storage, analysis, signature.ret)?;
            let ptr = builder.alloca_at_entry(IrType::Struct(name));
            builder.emit(Inst::Store {
                ptr,
                value,
                ty: IrType::Struct(name),
            });
            Ok(ptr)
        }
        ExprKind::Index { base, index } => {
            let elem_ty = analysis.expr_type(expr.span).unwrap_or(Type::Error);
            if !matches!(elem_ty, Type::Struct(_)) {
                return Err("indexed value is not a struct".to_string());
            }
            let base_ty = analysis.expr_type(base.span).unwrap_or(Type::Error);
            if let Type::DynArray(_) = base_ty {
                let operand =
                    lower_dyn_index(builder, base, index, expr, scopes, storage, analysis)?;
                match operand {
                    Operand::Value(value) => Ok(value),
                    _ => Err("expected a struct place".to_string()),
                }
            } else {
                let base_ptr = lower_array_place(builder, base, scopes, storage, analysis)?;
                let idx_value = lower_expr(builder, index, scopes, storage, analysis, Type::Error)?;
                let index_ty = analysis.expr_type(index.span).unwrap_or(Type::I64);
                let array_type = match base_ty {
                    Type::Array { llvm, display, .. } => {
                        if let Some(info) = analysis.array_info(display) {
                            lower_bounds_check(
                                builder,
                                idx_value.clone(),
                                index_ty,
                                Operand::Const(Const::Int(info.len as i64)),
                            );
                        }
                        llvm
                    }
                    _ => return Err("cannot index a non-array value".to_string()),
                };
                let dst = builder.new_value();
                builder.emit(Inst::IndexPtr {
                    dst,
                    base: base_ptr,
                    array_type,
                    index: idx_value,
                    elem_ty: IrType::from_type(elem_ty),
                });
                Ok(dst)
            }
        }
        ExprKind::Tuple(elements) => {
            let ty = analysis
                .expr_type(expr.span)
                .ok_or_else(|| "missing type information for tuple".to_string())?;
            let Type::Struct(name) = ty else {
                return Err("not a tuple value".to_string());
            };
            let info = analysis
                .struct_info(name)
                .ok_or_else(|| "missing tuple type information".to_string())?;
            let ptr = builder.alloca_at_entry(IrType::Struct(name));
            for (index, element) in elements.iter().enumerate() {
                let Some((_, field_ty)) = info.fields.get(index) else {
                    return Err("too many values for tuple".to_string());
                };
                let ir_ty = IrType::from_type(*field_ty);
                let value = lower_expr(builder, element, scopes, storage, analysis, *field_ty)?;
                let field_ptr = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: field_ptr,
                    base: ptr,
                    struct_name: name,
                    index: index as u32,
                    field_ty: ir_ty,
                });
                builder.emit(Inst::Store {
                    ptr: field_ptr,
                    value,
                    ty: ir_ty,
                });
            }
            Ok(ptr)
        }
        ExprKind::Field { base, .. } => {
            let Some((struct_name, index, field_ty)) = analysis.field_read(expr.span) else {
                return Err("missing field information during code generation".to_string());
            };
            if !matches!(field_ty, Type::Struct(_)) {
                return Err("field does not have a struct type".to_string());
            }
            let base_ptr = lower_struct_place(builder, base, scopes, storage, analysis)?;
            let field_ptr = builder.new_value();
            builder.emit(Inst::FieldPtr {
                dst: field_ptr,
                base: base_ptr,
                struct_name,
                index,
                field_ty: IrType::from_type(field_ty),
            });
            Ok(field_ptr)
        }
        _ => Err("struct values must be variables or constructor calls".to_string()),
    }
}

fn emit_print_str(builder: &mut FunctionBuilder, value: &str) {
    builder.emit(Inst::Call {
        dst: None,
        name: "nex_rt_print_str".to_string(),
        args: vec![Operand::Const(Const::Str(value.to_string()))],
        ret: IrType::Void,
    });
}

fn lower_print_scalar_place(
    builder: &mut FunctionBuilder,
    place: ValueId,
    ty: Type,
) -> Result<(), String> {
    let ir_ty = IrType::from_type(ty);
    let dst = builder.new_value();
    builder.emit(Inst::Load {
        dst,
        ptr: place,
        ty: ir_ty,
    });
    let (runtime_name, cast) = runtime_print_name("print", ty)?;
    let arg = if let Some((kind, from, to)) = cast {
        let cast_dst = builder.new_value();
        builder.emit(Inst::Cast {
            dst: cast_dst,
            kind,
            from,
            to,
            operand: Operand::Value(dst),
        });
        Operand::Value(cast_dst)
    } else {
        Operand::Value(dst)
    };
    builder.emit(Inst::Call {
        dst: None,
        name: runtime_name,
        args: vec![arg],
        ret: IrType::Void,
    });
    Ok(())
}

fn lower_print_value_place(
    builder: &mut FunctionBuilder,
    place: ValueId,
    ty: Type,
    analysis: &Analysis,
) -> Result<(), String> {
    match ty {
        Type::Struct(name) => {
            let tuple = name.starts_with("tuple_");
            if tuple {
                emit_print_str(builder, "(");
            } else {
                emit_print_str(builder, "{");
            }
            if let Some(info) = analysis.struct_info(name) {
                for (index, (_, field_ty)) in info.fields.iter().enumerate() {
                    if index > 0 {
                        emit_print_str(builder, ", ");
                    }
                    let field_ptr = builder.new_value();
                    builder.emit(Inst::FieldPtr {
                        dst: field_ptr,
                        base: place,
                        struct_name: name,
                        index: index as u32,
                        field_ty: IrType::from_type(*field_ty),
                    });
                    lower_print_value_place(builder, field_ptr, *field_ty, analysis)?;
                }
            }
            if name.starts_with("tuple_") {
                emit_print_str(builder, ")");
            } else {
                emit_print_str(builder, "}");
            }
        }
        Type::Array { display, .. } => {
            if let Some(info) = analysis.array_info(display) {
                let elem_ty = info.elem;
                let elem_ir = IrType::from_type(elem_ty);
                emit_print_str(builder, "[");
                for index in 0..info.len {
                    if index > 0 {
                        emit_print_str(builder, ", ");
                    }
                    let idx = Operand::Const(Const::Int(index as i64));
                    let elem_ptr = builder.new_value();
                    builder.emit(Inst::IndexPtr {
                        dst: elem_ptr,
                        base: place,
                        array_type: info.llvm,
                        index: idx,
                        elem_ty: elem_ir,
                    });
                    lower_print_value_place(builder, elem_ptr, elem_ty, analysis)?;
                }
                emit_print_str(builder, "]");
            }
        }
        _ => lower_print_scalar_place(builder, place, ty)?,
    }
    Ok(())
}

fn lower_print_aggregate(
    builder: &mut FunctionBuilder,
    place: ValueId,
    ty: Type,
    newline: bool,
    analysis: &Analysis,
) -> Result<(), String> {
    lower_print_value_place(builder, place, ty, analysis)?;
    if newline {
        emit_print_str(builder, "\n");
    }
    Ok(())
}

fn lower_scalar_place_equality(
    builder: &mut FunctionBuilder,
    lhs: ValueId,
    rhs: ValueId,
    ty: Type,
) -> Result<Operand, String> {
    match ty {
        Type::String => {
            let lhs_value = builder.new_value();
            builder.emit(Inst::Load {
                dst: lhs_value,
                ptr: lhs,
                ty: IrType::String,
            });
            let rhs_value = builder.new_value();
            builder.emit(Inst::Load {
                dst: rhs_value,
                ptr: rhs,
                ty: IrType::String,
            });
            let call_dst = builder.new_value();
            builder.emit(Inst::Call {
                dst: Some(call_dst),
                name: "nex_rt_str_eq".to_string(),
                args: vec![Operand::Value(lhs_value), Operand::Value(rhs_value)],
                ret: IrType::I32,
            });
            let dst = builder.new_value();
            builder.emit(Inst::Bin {
                dst,
                op: BinOp::Ne,
                ty: IrType::I32,
                lhs: Operand::Value(call_dst),
                rhs: Operand::Const(Const::Int(0)),
            });
            Ok(Operand::Value(dst))
        }
        _ => {
            let ir_ty = IrType::from_type(ty);
            let lhs_value = builder.new_value();
            builder.emit(Inst::Load {
                dst: lhs_value,
                ptr: lhs,
                ty: ir_ty,
            });
            let rhs_value = builder.new_value();
            builder.emit(Inst::Load {
                dst: rhs_value,
                ptr: rhs,
                ty: ir_ty,
            });
            let dst = builder.new_value();
            builder.emit(Inst::Bin {
                dst,
                op: BinOp::Eq,
                ty: ir_ty,
                lhs: Operand::Value(lhs_value),
                rhs: Operand::Value(rhs_value),
            });
            Ok(Operand::Value(dst))
        }
    }
}

fn lower_place_equality(
    builder: &mut FunctionBuilder,
    lhs: ValueId,
    rhs: ValueId,
    ty: Type,
    analysis: &Analysis,
) -> Result<Operand, String> {
    match ty {
        Type::Struct(name) => {
            let Some(info) = analysis.struct_info(name) else {
                return Err(format!("unknown struct `{name}` during code generation"));
            };
            let mut result: Option<Operand> = None;
            for (index, (_, field_ty)) in info.fields.iter().enumerate() {
                let lhs_field = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: lhs_field,
                    base: lhs,
                    struct_name: name,
                    index: index as u32,
                    field_ty: IrType::from_type(*field_ty),
                });
                let rhs_field = builder.new_value();
                builder.emit(Inst::FieldPtr {
                    dst: rhs_field,
                    base: rhs,
                    struct_name: name,
                    index: index as u32,
                    field_ty: IrType::from_type(*field_ty),
                });
                let cmp = lower_place_equality(builder, lhs_field, rhs_field, *field_ty, analysis)?;
                result = Some(match result {
                    None => cmp,
                    Some(previous) => {
                        let dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst,
                            op: BinOp::Mul,
                            ty: IrType::Bool,
                            lhs: previous,
                            rhs: cmp,
                        });
                        Operand::Value(dst)
                    }
                });
            }
            Ok(result.unwrap_or(Operand::Const(Const::Bool(true))))
        }
        Type::Array { display, .. } => {
            let Some(info) = analysis.array_info(display) else {
                return Err("missing array type information".to_string());
            };
            let elem_ty = info.elem;
            let elem_ir = IrType::from_type(elem_ty);
            let mut result: Option<Operand> = None;
            for index in 0..info.len {
                let idx = Operand::Const(Const::Int(index as i64));
                let lhs_elem = builder.new_value();
                builder.emit(Inst::IndexPtr {
                    dst: lhs_elem,
                    base: lhs,
                    array_type: info.llvm,
                    index: idx.clone(),
                    elem_ty: elem_ir,
                });
                let rhs_elem = builder.new_value();
                builder.emit(Inst::IndexPtr {
                    dst: rhs_elem,
                    base: rhs,
                    array_type: info.llvm,
                    index: idx,
                    elem_ty: elem_ir,
                });
                let cmp = lower_place_equality(builder, lhs_elem, rhs_elem, elem_ty, analysis)?;
                result = Some(match result {
                    None => cmp,
                    Some(previous) => {
                        let dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst,
                            op: BinOp::Mul,
                            ty: IrType::Bool,
                            lhs: previous,
                            rhs: cmp,
                        });
                        Operand::Value(dst)
                    }
                });
            }
            Ok(result.unwrap_or(Operand::Const(Const::Bool(true))))
        }
        _ => lower_scalar_place_equality(builder, lhs, rhs, ty),
    }
}

/// Emits frees for every growable-array field contained in the struct pointed
/// to by `base` (recursing into nested structs). Used by RAII at scope exit
/// and by struct reassignment.
/// Frees every growable-array field of a struct and replaces it with a fresh
/// empty header, so the struct stays usable and a later scope-exit free cannot
/// double-free. Recurses into nested struct fields.
fn lower_bounds_check(builder: &mut FunctionBuilder, index: Operand, index_ty: Type, len: Operand) {
    let ir_ty = IrType::from_type(index_ty);
    let ge_zero = builder.new_value();
    builder.emit(Inst::Bin {
        dst: ge_zero,
        op: BinOp::Ge,
        ty: ir_ty,
        lhs: index.clone(),
        rhs: Operand::Const(Const::Int(0)),
    });
    let lt_len = builder.new_value();
    builder.emit(Inst::Bin {
        dst: lt_len,
        op: BinOp::Lt,
        ty: ir_ty,
        lhs: index,
        rhs: len,
    });
    let ok = builder.new_value();
    builder.emit(Inst::Bin {
        dst: ok,
        op: BinOp::Mul,
        ty: IrType::Bool,
        lhs: Operand::Value(ge_zero),
        rhs: Operand::Value(lt_len),
    });

    let ok_bb = builder.new_block();
    let panic_bb = builder.new_block();
    builder.terminate(Terminator::CondBr {
        cond: Operand::Value(ok),
        then_bb: ok_bb,
        else_bb: panic_bb,
    });

    builder.current = panic_bb;
    builder.emit(Inst::Call {
        dst: None,
        name: "nex_rt_panic".to_string(),
        args: vec![Operand::Const(Const::Str(
            "array index out of bounds".to_string(),
        ))],
        ret: IrType::Void,
    });
    builder.terminate(Terminator::Unreachable);

    builder.current = ok_bb;
}

fn fold_bin(op: BinOp, lhs: &Const, rhs: &Const, ty: Type) -> Option<Const> {
    match (op, lhs, rhs) {
        (BinOp::Add, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_add(*b))),
        (BinOp::Sub, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_sub(*b))),
        (BinOp::Mul, Const::Int(a), Const::Int(b)) => Some(Const::Int(a.wrapping_mul(*b))),
        (BinOp::Div, Const::Int(a), Const::Int(b)) if *b != 0 => Some(Const::Int(a / *b)),
        (BinOp::Rem, Const::Int(a), Const::Int(b)) if *b != 0 => Some(Const::Int(a % *b)),
        (BinOp::Add, Const::Float(a), Const::Float(b)) => Some(Const::Float(a + b)),
        (BinOp::Sub, Const::Float(a), Const::Float(b)) => Some(Const::Float(a - b)),
        (BinOp::Mul, Const::Float(a), Const::Float(b)) => Some(Const::Float(a * b)),
        (BinOp::Div, Const::Float(a), Const::Float(b)) if *b != 0.0 => Some(Const::Float(a / b)),
        (BinOp::Rem, Const::Float(a), Const::Float(b)) if *b != 0.0 => Some(Const::Float(a % b)),
        (BinOp::Eq, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a != b)),
        (BinOp::Lt, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a < b)),
        (BinOp::Le, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a <= b)),
        (BinOp::Gt, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a > b)),
        (BinOp::Ge, Const::Int(a), Const::Int(b)) => Some(Const::Bool(a >= b)),
        (BinOp::Eq, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a != b)),
        (BinOp::Lt, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a < b)),
        (BinOp::Le, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a <= b)),
        (BinOp::Gt, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a > b)),
        (BinOp::Ge, Const::Float(a), Const::Float(b)) => Some(Const::Bool(a >= b)),
        (BinOp::Eq, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Bool(a), Const::Bool(b)) => Some(Const::Bool(a != b)),
        (BinOp::Eq, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a != b)),
        (BinOp::Lt, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a < b)),
        (BinOp::Le, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a <= b)),
        (BinOp::Gt, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a > b)),
        (BinOp::Ge, Const::Char(a), Const::Char(b)) => Some(Const::Bool(a >= b)),
        (BinOp::Eq, Const::Str(a), Const::Str(b)) => Some(Const::Bool(a == b)),
        (BinOp::Ne, Const::Str(a), Const::Str(b)) => Some(Const::Bool(a != b)),
        _ => None,
    }
    .filter(|_| ty != Type::Error)
}

fn lower_match(
    builder: &mut FunctionBuilder,
    scrutinee: &Expr,
    arms: &[nex_ast::MatchArm],
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    deferred: &mut Vec<(ValueId, Block)>,
) -> Result<(), String> {
    let scrut = lower_expr(builder, scrutinee, scopes, storage, analysis, Type::Error)?;
    let merge_bb = builder.new_block();
    let mut merge_reachable = false;
    let mut exhaustive = false;
    let mut pending: Option<BlockId> = Some(builder.new_block());
    let pending_bb = pending.unwrap();
    builder.terminate(Terminator::Br(pending_bb));
    builder.current = pending_bb;

    let scrut_ty = analysis.expr_type(scrutinee.span).unwrap_or(Type::Error);
    let scrut_ir = IrType::from_type(scrut_ty);

    for arm in arms {
        let body_bb = builder.new_block();
        match &arm.pattern {
            nex_ast::Pattern::Else => {
                builder.terminate(Terminator::Br(body_bb));
                builder.current = body_bb;
                scopes.push(HashMap::new());
                lower_stmts(
                    builder,
                    &arm.body.stmts,
                    scopes,
                    storage,
                    analysis,
                    deferred,
                )?;
                scopes.pop();
                if !builder.current_terminated() {
                    merge_reachable = true;
                    builder.terminate(Terminator::Br(merge_bb));
                }
                pending = None;
            }
            other => {
                let cond = match other {
                    nex_ast::Pattern::Variant(variant) => {
                        let tag = if let Type::Enum(enum_name) = scrut_ty {
                            analysis
                                .enum_info(enum_name)
                                .and_then(|info| {
                                    info.variants.iter().position(|value| value == variant)
                                })
                                .unwrap_or(0) as i64
                        } else {
                            0
                        };
                        let cmp_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: cmp_dst,
                            op: BinOp::Eq,
                            ty: IrType::I32,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Int(tag)),
                        });
                        Operand::Value(cmp_dst)
                    }
                    nex_ast::Pattern::Int(value) => {
                        let cmp_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: cmp_dst,
                            op: BinOp::Eq,
                            ty: scrut_ir,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Int(*value)),
                        });
                        Operand::Value(cmp_dst)
                    }
                    nex_ast::Pattern::Range(start, end) => {
                        let ge_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: ge_dst,
                            op: BinOp::Ge,
                            ty: scrut_ir,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Int(*start)),
                        });
                        let le_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: le_dst,
                            op: BinOp::Le,
                            ty: scrut_ir,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Int(*end)),
                        });
                        let and_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: and_dst,
                            op: BinOp::Mul,
                            ty: IrType::Bool,
                            lhs: Operand::Value(ge_dst),
                            rhs: Operand::Value(le_dst),
                        });
                        Operand::Value(and_dst)
                    }
                    nex_ast::Pattern::Bool(value) => {
                        let cmp_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: cmp_dst,
                            op: BinOp::Eq,
                            ty: scrut_ir,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Bool(*value)),
                        });
                        Operand::Value(cmp_dst)
                    }
                    nex_ast::Pattern::Char(value) => {
                        let cmp_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: cmp_dst,
                            op: BinOp::Eq,
                            ty: scrut_ir,
                            lhs: scrut.clone(),
                            rhs: Operand::Const(Const::Char(*value)),
                        });
                        Operand::Value(cmp_dst)
                    }
                    nex_ast::Pattern::Str(value) => {
                        let call_dst = builder.new_value();
                        builder.emit(Inst::Call {
                            dst: Some(call_dst),
                            name: "nex_rt_str_eq".to_string(),
                            args: vec![scrut.clone(), Operand::Const(Const::Str(value.clone()))],
                            ret: IrType::I32,
                        });
                        let cmp_dst = builder.new_value();
                        builder.emit(Inst::Bin {
                            dst: cmp_dst,
                            op: BinOp::Ne,
                            ty: IrType::I32,
                            lhs: Operand::Value(call_dst),
                            rhs: Operand::Const(Const::Int(0)),
                        });
                        Operand::Value(cmp_dst)
                    }
                    nex_ast::Pattern::Else => Operand::Const(Const::Bool(true)),
                };
                let mut cond = cond;

                if let Some(guard) = &arm.guard {
                    let guard_val =
                        lower_expr(builder, guard, scopes, storage, analysis, Type::Error)?;
                    let and_dst = builder.new_value();
                    builder.emit(Inst::Bin {
                        dst: and_dst,
                        op: BinOp::Mul,
                        ty: IrType::Bool,
                        lhs: cond,
                        rhs: guard_val,
                    });
                    cond = Operand::Value(and_dst);
                }

                let next_bb = builder.new_block();
                builder.terminate(Terminator::CondBr {
                    cond,
                    then_bb: body_bb,
                    else_bb: next_bb,
                });

                builder.current = body_bb;
                scopes.push(HashMap::new());
                lower_stmts(
                    builder,
                    &arm.body.stmts,
                    scopes,
                    storage,
                    analysis,
                    deferred,
                )?;
                scopes.pop();
                if !builder.current_terminated() {
                    merge_reachable = true;
                    builder.terminate(Terminator::Br(merge_bb));
                }

                pending = Some(next_bb);
                builder.current = next_bb;
            }
        }
    }

    if let Type::Enum(enum_name) = scrut_ty {
        if let Some(info) = analysis.enum_info(enum_name) {
            let mut seen = std::collections::HashSet::new();
            let mut else_seen = false;
            for arm in arms {
                match &arm.pattern {
                    nex_ast::Pattern::Variant(variant) => {
                        seen.insert(variant.clone());
                    }
                    nex_ast::Pattern::Else => {
                        else_seen = true;
                    }
                    _ => {}
                }
            }
            exhaustive = else_seen || seen.len() == info.variants.len();
        }
    } else if scrut_ty == Type::Bool {
        let mut seen = std::collections::HashSet::new();
        let mut else_seen = false;
        for arm in arms {
            match &arm.pattern {
                nex_ast::Pattern::Bool(value) => {
                    seen.insert(*value);
                }
                nex_ast::Pattern::Else => {
                    else_seen = true;
                }
                _ => {}
            }
        }
        exhaustive = else_seen || seen.len() == 2;
    } else {
        let mut else_seen = false;
        for arm in arms {
            if matches!(arm.pattern, nex_ast::Pattern::Else) {
                else_seen = true;
            }
        }
        exhaustive = else_seen;
    }

    if let Some(pending_bb) = pending {
        if exhaustive {
            if !builder
                .func
                .blocks
                .get(pending_bb.0 as usize)
                .and_then(|block| block.terminator.as_ref())
                .is_some()
            {
                builder.current = pending_bb;
                builder.terminate(Terminator::Unreachable);
            }
        } else {
            merge_reachable = true;
            if !builder
                .func
                .blocks
                .get(pending_bb.0 as usize)
                .and_then(|block| block.terminator.as_ref())
                .is_some()
            {
                builder.current = pending_bb;
                builder.terminate(Terminator::Br(merge_bb));
            }
        }
    }

    builder.current = merge_bb;
    if !merge_reachable {
        builder.terminate(Terminator::Unreachable);
    }
    Ok(())
}

fn lower_array_place(
    builder: &mut FunctionBuilder,
    expr: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<ValueId, String> {
    if let ExprKind::Field { base, name, .. } = &expr.kind
        && let ExprKind::Ident(module_name) = &base.kind
        && analysis.modules.contains(module_name)
    {
        let qualified = format!("{}.{}", module_name, name);
        if let Some(init) = analysis.const_exprs.get(&qualified) {
            return lower_array_place(builder, init, scopes, storage, analysis);
        }
    }
    match &expr.kind {
        ExprKind::Ident(name) => {
            let resolved = analysis
                .aliases
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone());
            if let Some(init) = analysis.const_exprs.get(&resolved) {
                return lower_array_place(builder, init, scopes, storage, analysis);
            }
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
            Ok(slot.ptr)
        }
        ExprKind::Deref(inner) => {
            // `(*r)` as a place: the base address the reference `r` holds.
            let op = lower_expr(builder, inner, scopes, storage, analysis, Type::Error)?;
            operand_value_id(builder, op)
        }
        ExprKind::Call { callee, .. } => {
            let resolved = analysis
                .aliases
                .get(callee)
                .cloned()
                .unwrap_or_else(|| callee.clone());
            let callee = resolved.as_str();
            let signature = analysis
                .mono_call(expr.span)
                .and_then(|mono_name| analysis.function(mono_name))
                .or_else(|| analysis.function(callee))
                .ok_or_else(|| format!("unknown function `{callee}` during code generation"))?;
            let Type::Array { llvm, display, .. } = signature.ret else {
                return Err(format!("`{callee}` does not return an array value"));
            };
            let _ = display;
            let value = lower_expr(builder, expr, scopes, storage, analysis, signature.ret)?;
            let ptr = builder.alloca_at_entry(IrType::Array(llvm));
            builder.emit(Inst::Store {
                ptr,
                value,
                ty: IrType::Array(llvm),
            });
            Ok(ptr)
        }
        ExprKind::Field { base, .. } => {
            let Some((struct_name, index, field_ty)) = analysis.field_read(expr.span) else {
                return Err("missing field information during code generation".to_string());
            };
            let field_ty = IrType::from_type(field_ty);
            let base_ptr = lower_array_place(builder, base, scopes, storage, analysis)?;
            let field_ptr = builder.new_value();
            builder.emit(Inst::FieldPtr {
                dst: field_ptr,
                base: base_ptr,
                struct_name,
                index,
                field_ty,
            });
            Ok(field_ptr)
        }
        ExprKind::Array(elements) => {
            let ty = analysis
                .expr_type(expr.span)
                .ok_or_else(|| "missing type information for array".to_string())?;
            if let Type::DynArray(display) = ty {
                return lower_dyn_array_literal(
                    builder, elements, display, analysis, scopes, storage,
                );
            }
            let Type::Array { llvm, display, .. } = ty else {
                return Err("not an array value".to_string());
            };
            let elem = analysis
                .array_info(display)
                .map(|info| info.elem)
                .unwrap_or(Type::Error);
            let elem_ir = IrType::from_type(elem);
            let ptr = builder.alloca_at_entry(IrType::Array(llvm));
            for (index, element) in elements.iter().enumerate() {
                let idx = Operand::Const(Const::Int(index as i64));
                let elem_ptr = builder.new_value();
                builder.emit(Inst::IndexPtr {
                    dst: elem_ptr,
                    base: ptr,
                    array_type: llvm,
                    index: idx,
                    elem_ty: elem_ir,
                });
                if let Type::Struct(struct_name) = elem {
                    let source = lower_struct_place(builder, element, scopes, storage, analysis)?;
                    copy_struct(builder, elem_ptr, source, struct_name, analysis)?;
                } else {
                    let value = lower_expr(builder, element, scopes, storage, analysis, elem)?;
                    builder.emit(Inst::Store {
                        ptr: elem_ptr,
                        value,
                        ty: elem_ir,
                    });
                }
            }
            Ok(ptr)
        }
        _ => Err("array values must be variables or array literals".to_string()),
    }
}

fn copy_array(
    builder: &mut FunctionBuilder,
    dst: ValueId,
    src: ValueId,
    array_type: &'static str,
    elem: Type,
    len: usize,
    analysis: &Analysis,
) -> Result<(), String> {
    let elem_ir = IrType::from_type(elem);
    if let Type::Struct(struct_name) = elem {
        for index in 0..len {
            let idx = Operand::Const(Const::Int(index as i64));
            let dst_field = builder.new_value();
            builder.emit(Inst::IndexPtr {
                dst: dst_field,
                base: dst,
                array_type,
                index: idx.clone(),
                elem_ty: elem_ir,
            });
            let src_field = builder.new_value();
            builder.emit(Inst::IndexPtr {
                dst: src_field,
                base: src,
                array_type,
                index: idx,
                elem_ty: elem_ir,
            });
            copy_struct(builder, dst_field, src_field, struct_name, analysis)?;
        }
        return Ok(());
    }
    for index in 0..len {
        let idx = Operand::Const(Const::Int(index as i64));
        let dst_field = builder.new_value();
        builder.emit(Inst::IndexPtr {
            dst: dst_field,
            base: dst,
            array_type,
            index: idx.clone(),
            elem_ty: elem_ir,
        });
        let src_field = builder.new_value();
        builder.emit(Inst::IndexPtr {
            dst: src_field,
            base: src,
            array_type,
            index: idx,
            elem_ty: elem_ir,
        });
        let value = builder.new_value();
        builder.emit(Inst::Load {
            dst: value,
            ptr: src_field,
            ty: elem_ir,
        });
        builder.emit(Inst::Store {
            ptr: dst_field,
            value: Operand::Value(value),
            ty: elem_ir,
        });
    }
    Ok(())
}

/// Lowers a growable array literal into a heap header `{ data, len, cap }`.
fn lower_dyn_array_literal(
    builder: &mut FunctionBuilder,
    elements: &[Expr],
    display: &'static str,
    analysis: &Analysis,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
) -> Result<ValueId, String> {
    let elem = analysis
        .array_info(display)
        .map(|info| info.elem)
        .ok_or_else(|| "missing element type for growable array".to_string())?;
    let elem_ir = IrType::from_type(elem);
    let count = elements.len() as i64;
    let bytes = count.saturating_mul(ir_elem_size(analysis, elem_ir).max(1));

    let data_value = builder.new_value();
    builder.emit(Inst::Call {
        dst: Some(data_value),
        name: "nex_rt_array_alloc".to_string(),
        args: vec![Operand::Const(Const::Int(bytes))],
        ret: IrType::String,
    });

    let header = builder.new_value();
    builder.emit(Inst::Call {
        dst: Some(header),
        name: "nex_rt_array_new".to_string(),
        args: Vec::new(),
        ret: IrType::String,
    });

    // data
    let data_slot = field_ptr(builder, header, 0, IrType::String);
    builder.emit(Inst::Store {
        ptr: data_slot,
        value: Operand::Value(data_value),
        ty: IrType::String,
    });
    // len
    let len_slot = field_ptr(builder, header, 1, IrType::I64);
    builder.emit(Inst::Store {
        ptr: len_slot,
        value: Operand::Const(Const::Int(count)),
        ty: IrType::I64,
    });
    // cap
    let cap_slot = field_ptr(builder, header, 2, IrType::I64);
    builder.emit(Inst::Store {
        ptr: cap_slot,
        value: Operand::Const(Const::Int(count)),
        ty: IrType::I64,
    });

    for (index, element) in elements.iter().enumerate() {
        let elem_ptr_value = builder.new_value();
        builder.emit(Inst::DynIndexPtr {
            dst: elem_ptr_value,
            base: data_value,
            index: Operand::Const(Const::Int(index as i64)),
            elem_ty: elem_ir,
        });
        if let Type::Struct(struct_name) = elem {
            let source = lower_struct_place(builder, element, scopes, storage, analysis)?;
            copy_struct(builder, elem_ptr_value, source, struct_name, analysis)?;
        } else {
            let value = lower_expr(builder, element, scopes, storage, analysis, elem)?;
            builder.emit(Inst::Store {
                ptr: elem_ptr_value,
                value,
                ty: elem_ir,
            });
        }
    }

    Ok(header)
}

/// Pointer to a field of the growable array header.
fn field_ptr(
    builder: &mut FunctionBuilder,
    header: ValueId,
    index: i64,
    elem_ty: IrType,
) -> ValueId {
    let slot = builder.new_value();
    let _ = elem_ty;
    builder.emit(Inst::HeaderPtr {
        dst: slot,
        base: header,
        index: index as u32,
    });
    slot
}

/// Size in bytes of an IR type when stored in memory.
fn ir_elem_size(analysis: &Analysis, ty: IrType) -> i64 {
    match ty {
        IrType::I8 | IrType::U8 | IrType::Bool | IrType::Char => 1,
        IrType::I16 | IrType::U16 => 2,
        IrType::I32 | IrType::U32 | IrType::F32 => 4,
        IrType::I64 | IrType::U64 | IrType::F64 | IrType::String | IrType::Array(_) => 8,
        IrType::Struct(name) => {
            // Compute struct size from field types.
            if let Some(info) = analysis.struct_info(name) {
                let mut size: i64 = 0;
                for (_, field_ty) in &info.fields {
                    size += ir_elem_size(analysis, IrType::from_type(*field_ty));
                }
                // Align to 8 bytes for arrays.
                if size > 0 && size % 8 != 0 {
                    size += 8 - (size % 8);
                }
                size.max(8)
            } else {
                8
            }
        }
        IrType::Void => 0,
    }
}

/// Lowers `a[i]` for a growable array: bounds-check against the stored length,
/// then load from the data buffer.
/// Resolves an interned type name (from `Type::Ref { inner }`) back to a `Type`.
fn resolve_named_type(analysis: &Analysis, name: &str) -> Type {
    if let Some(resolved) = Type::from_name(name) {
        return resolved;
    }
    if let Some(info) = analysis.structs.get(name) {
        return Type::Struct(info.name_static);
    }
    if let Some(info) = analysis.arrays.get(name) {
        if info.len == 0 {
            return Type::DynArray(info.display);
        }
        return Type::Array {
            display: info.display,
            llvm: info.llvm,
        };
    }
    if let Some(info) = analysis.enums.get(name) {
        return Type::Enum(info.name_static);
    }
    Type::Error
}

fn lower_dyn_index(
    builder: &mut FunctionBuilder,
    base: &Expr,
    index: &Expr,
    expr: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<Operand, String> {
    let header_operand = lower_expr(builder, base, scopes, storage, analysis, Type::Error)?;
    let header = operand_value_id(builder, header_operand)?;
    let elem_ty = analysis.expr_type(expr.span).unwrap_or(Type::Error);
    let elem_ir = IrType::from_type(elem_ty);

    let len_slot = field_ptr(builder, header, 1, IrType::I64);
    let len_value = builder.new_value();
    builder.emit(Inst::Load {
        dst: len_value,
        ptr: len_slot,
        ty: IrType::I64,
    });

    let index_value = lower_expr(builder, index, scopes, storage, analysis, Type::Error)?;
    let index_ty = analysis.expr_type(index.span).unwrap_or(Type::I64);
    lower_bounds_check(
        builder,
        index_value.clone(),
        index_ty,
        Operand::Value(len_value),
    );

    let data_slot = field_ptr(builder, header, 0, IrType::String);
    let data_value = builder.new_value();
    builder.emit(Inst::Load {
        dst: data_value,
        ptr: data_slot,
        ty: IrType::String,
    });

    let elem_ptr = builder.new_value();
    builder.emit(Inst::DynIndexPtr {
        dst: elem_ptr,
        base: data_value,
        index: index_value,
        elem_ty: elem_ir,
    });

    if matches!(elem_ty, Type::Struct(_)) {
        return Ok(Operand::Value(elem_ptr));
    }

    let loaded = builder.new_value();
    builder.emit(Inst::Load {
        dst: loaded,
        ptr: elem_ptr,
        ty: elem_ir,
    });
    Ok(Operand::Value(loaded))
}

/// Lowers `a[i] = value` for a growable array.
#[allow(clippy::too_many_arguments)]
fn lower_dyn_assign_index(
    builder: &mut FunctionBuilder,
    header: ValueId,
    index: &Expr,
    value: &Expr,
    elem_ty: Type,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<(), String> {
    if elem_ty == Type::Error {
        return Err("cannot assign to an element of a growable array without a type".to_string());
    }
    let elem_ir = IrType::from_type(elem_ty);
    let len_slot = field_ptr(builder, header, 1, IrType::I64);
    let len_value = builder.new_value();
    builder.emit(Inst::Load {
        dst: len_value,
        ptr: len_slot,
        ty: IrType::I64,
    });
    let idx_value = lower_expr(builder, index, scopes, storage, analysis, Type::Error)?;
    lower_bounds_check(
        builder,
        idx_value.clone(),
        Type::I64,
        Operand::Value(len_value),
    );
    let data_slot = field_ptr(builder, header, 0, IrType::String);
    let data = builder.new_value();
    builder.emit(Inst::Load {
        dst: data,
        ptr: data_slot,
        ty: IrType::String,
    });
    let target = builder.new_value();
    builder.emit(Inst::DynIndexPtr {
        dst: target,
        base: data,
        index: idx_value,
        elem_ty: elem_ir,
    });
    if let Type::Struct(struct_name) = elem_ty {
        let source = lower_struct_place(builder, value, scopes, storage, analysis)?;
        copy_struct(builder, target, source, struct_name, analysis)?;
        return Ok(());
    }
    let value = lower_expr(builder, value, scopes, storage, analysis, elem_ty)?;
    builder.emit(Inst::Store {
        ptr: target,
        value,
        ty: elem_ir,
    });
    Ok(())
}

/// Materialises an operand into a value id when it is not already one.
fn operand_value_id(builder: &mut FunctionBuilder, operand: Operand) -> Result<ValueId, String> {
    match operand {
        Operand::Value(value) => Ok(value),
        other => {
            let slot = builder.alloca_at_entry(IrType::String);
            builder.emit(Inst::Store {
                ptr: slot,
                value: other,
                ty: IrType::String,
            });
            let loaded = builder.new_value();
            builder.emit(Inst::Load {
                dst: loaded,
                ptr: slot,
                ty: IrType::String,
            });
            Ok(loaded)
        }
    }
}

/// Loads a module-level global from its runtime slot.
fn lower_global_get(builder: &mut FunctionBuilder, slot: i64, ty: Type) -> Result<Operand, String> {
    let slot_operand = Operand::Const(Const::Int(slot));
    let dst = builder.new_value();
    let (name, ret) = match ty {
        Type::F64 => ("nex_rt_global_get_f64", IrType::F64),
        Type::String => ("nex_rt_global_get_str", IrType::String),
        Type::Bool => ("nex_rt_global_get_i64", IrType::I64),
        _ => ("nex_rt_global_get_i64", IrType::I64),
    };
    builder.emit(Inst::Call {
        dst: Some(dst),
        name: name.to_string(),
        args: vec![slot_operand],
        ret,
    });
    if ty == Type::Bool {
        let narrow = builder.new_value();
        builder.emit(Inst::Cast {
            dst: narrow,
            kind: CastKind::Truncate,
            from: IrType::I64,
            to: IrType::Bool,
            operand: Operand::Value(dst),
        });
        return Ok(Operand::Value(narrow));
    }
    Ok(Operand::Value(dst))
}

/// Stores a module-level global into its runtime slot.
fn lower_global_set(builder: &mut FunctionBuilder, slot: i64, ty: Type, value: Operand) {
    let slot_operand = Operand::Const(Const::Int(slot));
    let (name, cast) = match ty {
        Type::F64 => ("nex_rt_global_set_f64", None),
        Type::String => ("nex_rt_global_set_str", None),
        _ => ("nex_rt_global_set_i64", Some(true)),
    };
    let value = if cast == Some(true) && ty == Type::Bool {
        let wide = builder.new_value();
        builder.emit(Inst::Cast {
            dst: wide,
            kind: CastKind::ZeroExtend,
            from: IrType::Bool,
            to: IrType::I64,
            operand: value,
        });
        Operand::Value(wide)
    } else {
        value
    };
    builder.emit(Inst::Call {
        dst: None,
        name: name.to_string(),
        args: vec![slot_operand, value],
        ret: IrType::Void,
    });
}

include!("lower_expr.rs");

include!("copy_helpers.rs");
