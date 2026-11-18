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
        }
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
    for block in &program.impls {
        for method in &block.methods {
            let mut method = method.clone();
            method.name = format!("{}.{}", block.type_name, method.name);
            functions.push(method);
        }
    }

    for function in &functions {
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
        for (param, (value, ty)) in function.params.iter().zip(ir_params.iter()) {
            // Parameters are stored in stack slots so that struct parameters
            // (and scalar parameters) are addressable and use value semantics.
            let ptr = builder.alloca_at_entry(*ty);
            builder.emit(Inst::Store {
                ptr,
                value: Operand::Value(*value),
                ty: *ty,
            });
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
            storage.insert(span, (ptr, ir_ty));
        }

        scopes.push(HashMap::new());
        lower_stmts(
            &mut builder,
            &function.body.stmts,
            &mut scopes,
            &storage,
            analysis,
        )?;
        scopes.pop();

        if !builder.current_terminated() {
            if ret == IrType::Void {
                builder.terminate(Terminator::Ret(None));
            } else {
                return Err(format!(
                    "function `{}` may exit without returning a value",
                    function.name
                ));
            }
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
) -> Result<(), String> {
    for statement in statements {
        if builder.current_terminated() {
            break;
        }
        lower_statement(builder, statement, scopes, storage, analysis)?;
    }
    Ok(())
}

fn lower_statement(
    builder: &mut FunctionBuilder,
    statement: &Stmt,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<(), String> {
    match &statement.kind {
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
                copy_array(builder, ptr, source, info.llvm, info.elem, info.len)?;
            } else {
                let value = lower_expr(builder, init, scopes, storage, analysis, expected)?;
                builder.emit(Inst::Store { ptr, value, ty });
            }
            if let Some(scope) = scopes.last_mut() {
                scope.insert(name.clone(), Slot { ptr, ty });
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
                    copy_struct(builder, ptr, source, struct_name, analysis)?;
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
                    copy_array(builder, ptr, source, array_type, elem, len)?;
                    Ok(())
                }
                (ptr, ty) => {
                    let operand =
                        lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                    builder.emit(Inst::Store {
                        ptr,
                        value: operand,
                        ty,
                    });
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
        } => lower_if(builder, branches, else_branch, scopes, storage, analysis),
        StmtKind::While { cond, body } => {
            lower_while(builder, cond, body, scopes, storage, analysis)
        }
        StmtKind::For {
            var,
            start,
            end,
            body,
        } => lower_for(builder, var, (start, end), body, scopes, storage, analysis),
        StmtKind::Match { scrutinee, arms } => {
            lower_match(builder, scrutinee, arms, scopes, storage, analysis)
        }
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
            let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
            builder.emit(Inst::Store {
                ptr: dst_ptr,
                value: operand,
                ty: elem_ir,
            });
            Ok(())
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                let operand = lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                builder.terminate(Terminator::Ret(Some(operand)));
            } else {
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
        StmtKind::Unsafe(block) => lower_stmts(builder, &block.stmts, scopes, storage, analysis),
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
        lower_stmts(builder, &body.stmts, scopes, storage, analysis)?;
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
        lower_stmts(builder, &body.stmts, scopes, storage, analysis)?;
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
    lower_stmts(builder, &body.stmts, scopes, storage, analysis)?;
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

fn lower_for(
    builder: &mut FunctionBuilder,
    var: &str,
    range: (&Expr, &Expr),
    body: &Block,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
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
    lower_stmts(builder, &body.stmts, scopes, storage, analysis)?;
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

            let resolved = analysis
                .aliases
                .get(callee)
                .cloned()
                .unwrap_or_else(|| callee.clone());
            let callee = resolved.as_str();
            if let Some(info) = analysis.struct_info(callee) {
                let ptr = builder.alloca_at_entry(IrType::Struct(info.name_static));
                for (index, arg) in args.iter().enumerate() {
                    let Some((_, field_ty)) = info.fields.get(index) else {
                        return Err(format!("too many values for struct `{callee}`"));
                    };
                    let ir_ty = IrType::from_type(*field_ty);
                    let value = lower_expr(builder, arg, scopes, storage, analysis, *field_ty)?;
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
                .function(callee)
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

fn copy_struct(
    builder: &mut FunctionBuilder,
    dst: ValueId,
    src: ValueId,
    struct_name: &'static str,
    analysis: &Analysis,
) -> Result<(), String> {
    let info = analysis
        .struct_info(struct_name)
        .ok_or_else(|| format!("unknown struct `{struct_name}` during code generation"))?;
    for (index, (_, field_ty)) in info.fields.iter().enumerate() {
        let ir_ty = IrType::from_type(*field_ty);
        let dst_field = builder.new_value();
        builder.emit(Inst::FieldPtr {
            dst: dst_field,
            base: dst,
            struct_name,
            index: index as u32,
            field_ty: ir_ty,
        });
        let src_field = builder.new_value();
        builder.emit(Inst::FieldPtr {
            dst: src_field,
            base: src,
            struct_name,
            index: index as u32,
            field_ty: ir_ty,
        });
        let value = builder.new_value();
        builder.emit(Inst::Load {
            dst: value,
            ptr: src_field,
            ty: ir_ty,
        });
        builder.emit(Inst::Store {
            ptr: dst_field,
            value: Operand::Value(value),
            ty: ir_ty,
        });
    }
    Ok(())
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
                lower_stmts(builder, &arm.body.stmts, scopes, storage, analysis)?;
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
                    let guard_val = lower_expr(builder, guard, scopes, storage, analysis, Type::Error)?;
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
                lower_stmts(builder, &arm.body.stmts, scopes, storage, analysis)?;
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
        ExprKind::Call { callee, .. } => {
            let resolved = analysis
                .aliases
                .get(callee)
                .cloned()
                .unwrap_or_else(|| callee.clone());
            let callee = resolved.as_str();
            let signature = analysis
                .function(callee)
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
                let value = lower_expr(builder, element, scopes, storage, analysis, elem)?;
                builder.emit(Inst::Store {
                    ptr: elem_ptr,
                    value,
                    ty: elem_ir,
                });
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
) -> Result<(), String> {
    let elem_ir = IrType::from_type(elem);
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
    let bytes = count.saturating_mul(ir_elem_size(elem_ir).max(1));

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
        let value = lower_expr(builder, element, scopes, storage, analysis, elem)?;
        builder.emit(Inst::Store {
            ptr: elem_ptr_value,
            value,
            ty: elem_ir,
        });
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
fn ir_elem_size(ty: IrType) -> i64 {
    match ty {
        IrType::I8 | IrType::U8 | IrType::Bool | IrType::Char => 1,
        IrType::I16 | IrType::U16 => 2,
        IrType::I32 | IrType::U32 | IrType::F32 => 4,
        IrType::I64 | IrType::U64 | IrType::F64 | IrType::String | IrType::Array(_) => 8,
        IrType::Struct(_) => 8,
        IrType::Void => 0,
    }
}

/// Lowers `a[i]` for a growable array: bounds-check against the stored length,
/// then load from the data buffer.
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

fn lower_expr(
    builder: &mut FunctionBuilder,
    expr: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
    _expected: Type,
) -> Result<Operand, String> {
    let expr_ty = analysis.expr_type(expr.span).unwrap_or(Type::Error);
    let ir_ty = IrType::from_type(expr_ty);

    match &expr.kind {
        ExprKind::Int(value) => Ok(Operand::Const(crate::types::Const::Int(*value))),
        ExprKind::Float(value) => Ok(Operand::Const(crate::types::Const::Float(*value))),
        ExprKind::Str(value) => Ok(Operand::Const(crate::types::Const::Str(value.clone()))),
        ExprKind::Char(value) => Ok(Operand::Const(crate::types::Const::Char(*value))),
        ExprKind::Bool(value) => Ok(Operand::Const(crate::types::Const::Bool(*value))),
        ExprKind::Ident(name) => {
            if let Some((_, index)) = analysis.variant_value(expr.span) {
                return Ok(Operand::Const(Const::Int(index as i64)));
            }
            let resolved = analysis
                .aliases
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone());
            if let Some(init) = analysis.const_exprs.get(&resolved) {
                return lower_expr(builder, init, scopes, storage, analysis, Type::Error);
            }
            if let Some(global) = analysis.globals.get(name) {
                return lower_global_get(builder, global.slot, global.ty);
            }
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
            let dst = builder.new_value();
            builder.emit(Inst::Load {
                dst,
                ptr: slot.ptr,
                ty: slot.ty,
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Unary { op, operand } => {
            let operand_value =
                lower_expr(builder, operand, scopes, storage, analysis, Type::Error)?;
            let dst = builder.new_value();
            let un_op = match op {
                nex_ast::UnaryOp::Neg => UnOp::Neg,
                nex_ast::UnaryOp::Not => UnOp::Not,
            };
            builder.emit(Inst::Un {
                dst,
                op: un_op,
                ty: ir_ty,
                operand: operand_value,
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Binary { op, left, right } => {
            if *op == nex_ast::BinOp::And {
                return lower_and(builder, left, right, scopes, storage, analysis);
            }
            if *op == nex_ast::BinOp::Or {
                return lower_or(builder, left, right, scopes, storage, analysis);
            }

            let bin_op = BinOp::from_ast(*op)
                .ok_or_else(|| "logical operators must be lowered separately".to_string())?;
            let ty = analysis.expr_type(left.span).unwrap_or(Type::Error);
            let operand_ty = IrType::from_type(ty);

            if matches!(bin_op, BinOp::Eq | BinOp::Ne)
                && matches!(ty, Type::Struct(_) | Type::Array { .. })
            {
                let lhs_place = if let Type::Struct(_) = ty {
                    lower_struct_place(builder, left, scopes, storage, analysis)?
                } else {
                    lower_array_place(builder, left, scopes, storage, analysis)?
                };
                let rhs_place = if let Type::Struct(_) = ty {
                    lower_struct_place(builder, right, scopes, storage, analysis)?
                } else {
                    lower_array_place(builder, right, scopes, storage, analysis)?
                };
                let equal = lower_place_equality(builder, lhs_place, rhs_place, ty, analysis)?;
                if bin_op == BinOp::Ne {
                    let dst = builder.new_value();
                    builder.emit(Inst::Un {
                        dst,
                        op: UnOp::Not,
                        ty: IrType::Bool,
                        operand: equal,
                    });
                    return Ok(Operand::Value(dst));
                }
                return Ok(equal);
            }

            if (bin_op == BinOp::Div || bin_op == BinOp::Rem) && operand_ty.is_integer() {
                let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
                let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
                let cmp_dst = builder.new_value();
                builder.emit(Inst::Bin {
                    dst: cmp_dst,
                    op: BinOp::Eq,
                    ty: operand_ty,
                    lhs: rhs.clone(),
                    rhs: Operand::Const(Const::Int(0)),
                });

                let panic_bb = builder.new_block();
                let ok_bb = builder.new_block();
                builder.terminate(Terminator::CondBr {
                    cond: Operand::Value(cmp_dst),
                    then_bb: panic_bb,
                    else_bb: ok_bb,
                });

                builder.current = panic_bb;
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_panic".to_string(),
                    args: vec![Operand::Const(Const::Str("division by zero".to_string()))],
                    ret: IrType::Void,
                });
                builder.terminate(Terminator::Unreachable);

                builder.current = ok_bb;
                let dst = builder.new_value();
                builder.emit(Inst::Bin {
                    dst,
                    op: bin_op,
                    ty: operand_ty,
                    lhs,
                    rhs,
                });
                return Ok(Operand::Value(dst));
            }

            if operand_ty == IrType::String && matches!(bin_op, BinOp::Eq | BinOp::Ne) {
                let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
                let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
                let call_dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(call_dst),
                    name: "nex_rt_str_eq".to_string(),
                    args: vec![lhs, rhs],
                    ret: IrType::I32,
                });
                // `nex_rt_str_eq` returns 1 for equal strings. Compare the
                // result with zero to produce the requested boolean.
                let cmp_op = if bin_op == BinOp::Eq {
                    BinOp::Ne
                } else {
                    BinOp::Eq
                };
                let dst = builder.new_value();
                builder.emit(Inst::Bin {
                    dst,
                    op: cmp_op,
                    ty: IrType::I32,
                    lhs: Operand::Value(call_dst),
                    rhs: Operand::Const(Const::Int(0)),
                });
                return Ok(Operand::Value(dst));
            }

            if operand_ty == IrType::String
                && matches!(bin_op, BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge)
            {
                let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
                let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
                let call_dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(call_dst),
                    name: "nex_rt_str_cmp".to_string(),
                    args: vec![lhs, rhs],
                    ret: IrType::I32,
                });
                // `nex_rt_str_cmp` returns a negative, zero, or positive value;
                // compare it with zero to produce the requested boolean.
                let dst = builder.new_value();
                builder.emit(Inst::Bin {
                    dst,
                    op: bin_op,
                    ty: IrType::I32,
                    lhs: Operand::Value(call_dst),
                    rhs: Operand::Const(Const::Int(0)),
                });
                return Ok(Operand::Value(dst));
            }

            let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
            let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
            if let (Operand::Const(left_const), Operand::Const(right_const)) = (&lhs, &rhs) {
                if let Some(result) = fold_bin(bin_op, left_const, right_const, ty) {
                    return Ok(Operand::Const(result));
                }
            }

            let dst = builder.new_value();
            builder.emit(Inst::Bin {
                dst,
                op: bin_op,
                ty: operand_ty,
                lhs,
                rhs,
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Call { callee, args, .. } => {
            if let Some(method) = analysis.method_call(expr.span).cloned() {
                let receiver_expr = Expr {
                    kind: ExprKind::Ident(method.receiver.clone()),
                    span: expr.span,
                };
                let receiver = lower_expr(
                    builder,
                    &receiver_expr,
                    scopes,
                    storage,
                    analysis,
                    method.receiver_ty,
                )?;
                let mut call_args = vec![receiver];
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let ret = IrType::from_type(method.ret);
                let dst = if ret == IrType::Void {
                    None
                } else {
                    Some(builder.new_value())
                };
                builder.emit(Inst::Call {
                    dst,
                    name: method.symbol,
                    args: call_args,
                    ret,
                });
                return if let Some(dst) = dst {
                    Ok(Operand::Value(dst))
                } else {
                    Ok(Operand::Const(Const::Bool(false)))
                };
            }

            let callee = analysis
                .aliases
                .get(callee)
                .cloned()
                .unwrap_or_else(|| callee.clone());
            let callee = callee.as_str();
            if callee == "print" || callee == "println" {
                if args.len() != 1 {
                    return Err(format!("`{callee}` expects exactly one argument"));
                }
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                if matches!(arg_ty, Type::Struct(_) | Type::Array { .. }) {
                    let place = if let Type::Struct(_) = arg_ty {
                        lower_struct_place(builder, &args[0], scopes, storage, analysis)?
                    } else {
                        lower_array_place(builder, &args[0], scopes, storage, analysis)?
                    };
                    lower_print_aggregate(builder, place, arg_ty, callee == "println", analysis)?;
                    return Ok(Operand::Const(Const::Bool(false)));
                }
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let (runtime_name, cast) = runtime_print_name(callee, arg_ty)?;
                let call_arg = if let Some((kind, from, to)) = cast {
                    let dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst,
                        kind,
                        from,
                        to,
                        operand: arg_value,
                    });
                    Operand::Value(dst)
                } else {
                    arg_value
                };
                builder.emit(Inst::Call {
                    dst: None,
                    name: runtime_name,
                    args: vec![call_arg],
                    ret: IrType::Void,
                });
                return Ok(Operand::Const(Const::Bool(false)));
            }

            if let Some(info) = analysis.struct_info(callee) {
                let ptr = lower_struct_place(builder, expr, scopes, storage, analysis)?;
                let dst = builder.new_value();
                builder.emit(Inst::Load {
                    dst,
                    ptr,
                    ty: IrType::Struct(info.name_static),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "len" {
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                if let Type::Array { display, .. } = arg_ty {
                    if let Some(info) = analysis.array_info(display) {
                        return Ok(Operand::Const(Const::Int(info.len as i64)));
                    }
                }
                if let Type::DynArray(_) = arg_ty {
                    let header_operand =
                        lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                    let header = operand_value_id(builder, header_operand)?;
                    let slot = field_ptr(builder, header, 1, IrType::I64);
                    let loaded = builder.new_value();
                    builder.emit(Inst::Load {
                        dst: loaded,
                        ptr: slot,
                        ty: IrType::I64,
                    });
                    return Ok(Operand::Value(loaded));
                }
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_len".to_string(),
                    args: vec![arg_value],
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_split_count" || callee == "str_split_at" || callee == "json_stringify"
            {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "str_split_count" => ("nex_rt_str_split_count", IrType::I64),
                    "str_split_at" => ("nex_rt_str_split_at", IrType::String),
                    _ => ("nex_rt_json_stringify", IrType::String),
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "thread_spawn" {
                let name = match &args[0].kind {
                    ExprKind::Str(name) => name.clone(),
                    _ => return Err("`thread_spawn` expects a function name literal".to_string()),
                };
                let dst = builder.new_value();
                builder.emit(Inst::FuncPtr {
                    dst,
                    name: format!("nex.fn.{name}"),
                });
                let mut call_args = vec![Operand::Value(dst)];
                if let Some(argument) = args.get(1) {
                    call_args.push(lower_expr(
                        builder,
                        argument,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let runtime = if args.len() == 2 {
                    "nex_rt_thread_spawn_arg"
                } else {
                    "nex_rt_thread_spawn"
                };
                let handle = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(handle),
                    name: runtime.to_string(),
                    args: call_args,
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(handle));
            }

            if callee == "thread_join" || callee == "thread_running" {
                let handle = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let name = if callee == "thread_join" {
                    "nex_rt_thread_join"
                } else {
                    "nex_rt_thread_running"
                };
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: name.to_string(),
                    args: vec![handle],
                    ret: IrType::I64,
                });
                let dst = builder.new_value();
                builder.emit(Inst::Cast {
                    dst,
                    kind: CastKind::Truncate,
                    from: IrType::I64,
                    to: IrType::Bool,
                    operand: Operand::Value(raw),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "mutex_new" || callee == "mutex_lock" || callee == "mutex_unlock" {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "mutex_new" => ("nex_rt_mutex_new", IrType::I64),
                    "mutex_lock" => ("nex_rt_mutex_lock", IrType::Void),
                    _ => ("nex_rt_mutex_unlock", IrType::Void),
                };
                if ret == IrType::Void {
                    builder.emit(Inst::Call {
                        dst: None,
                        name: name.to_string(),
                        args: call_args,
                        ret,
                    });
                    return Ok(Operand::Const(Const::Bool(true)));
                }
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                return Ok(Operand::Value(dst));
            }

            if callee.starts_with("router_") {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "router_new" => ("nex_rt_router_new", IrType::I64),
                    "router_add" => ("nex_rt_router_add", IrType::I64),
                    "router_match" => ("nex_rt_router_match", IrType::I64),
                    _ => ("nex_rt_router_param", IrType::String),
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "pool_new" {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_pool_new".to_string(),
                    args: call_args,
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "pool_submit" {
                let pool = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let name = match &args[1].kind {
                    ExprKind::Str(name) => name.clone(),
                    _ => return Err("`pool_submit` expects a function name literal".to_string()),
                };
                let argument =
                    lower_expr(builder, &args[2], scopes, storage, analysis, Type::Error)?;
                let entry = builder.new_value();
                builder.emit(Inst::FuncPtr {
                    dst: entry,
                    name: format!("nex.fn.{name}"),
                });
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: "nex_rt_pool_submit".to_string(),
                    args: vec![pool, Operand::Value(entry), argument],
                    ret: IrType::I32,
                });
                let dst = builder.new_value();
                builder.emit(Inst::Cast {
                    dst,
                    kind: CastKind::Truncate,
                    from: IrType::I32,
                    to: IrType::Bool,
                    operand: Operand::Value(raw),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "floor" || callee == "ceil" || callee == "round" || callee == "trunc" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let name = match callee {
                    "floor" => "nex_rt_floor_f64",
                    "ceil" => "nex_rt_ceil_f64",
                    "round" => "nex_rt_round_f64",
                    _ => "nex_rt_trunc_f64",
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![value],
                    ret: IrType::F64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "json_escape" || callee == "json_quote" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let name = if callee == "json_escape" {
                    "nex_rt_json_escape"
                } else {
                    "nex_rt_json_quote"
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![value],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee.starts_with("builder_") {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "builder_new" => ("nex_rt_builder_new", IrType::I64),
                    "builder_append" => ("nex_rt_builder_append", IrType::I32),
                    "builder_len" => ("nex_rt_builder_len", IrType::I64),
                    "builder_string" => ("nex_rt_builder_string", IrType::String),
                    _ => ("nex_rt_builder_clear", IrType::Void),
                };
                if ret == IrType::Void {
                    builder.emit(Inst::Call {
                        dst: None,
                        name: name.to_string(),
                        args: call_args,
                        ret,
                    });
                    return Ok(Operand::Const(Const::Bool(true)));
                }
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                if callee == "builder_append" {
                    let dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst,
                        kind: CastKind::Truncate,
                        from: IrType::I32,
                        to: IrType::Bool,
                        operand: Operand::Value(raw),
                    });
                    return Ok(Operand::Value(dst));
                }
                return Ok(Operand::Value(raw));
            }

            if callee.starts_with("map_") {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "map_new" => ("nex_rt_map_new", IrType::I64),
                    "map_set" => ("nex_rt_map_set", IrType::I64),
                    "map_get" => ("nex_rt_map_get", IrType::String),
                    "map_has" => ("nex_rt_map_has", IrType::I64),
                    "map_remove" => ("nex_rt_map_remove", IrType::I64),
                    "map_len" => ("nex_rt_map_len", IrType::I64),
                    _ => ("nex_rt_map_key_at", IrType::String),
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                if callee == "map_set" {
                    // map_set reports success; NEX sees it as a bool.
                    let bool_dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst: bool_dst,
                        kind: CastKind::Truncate,
                        from: IrType::I64,
                        to: IrType::Bool,
                        operand: Operand::Value(dst),
                    });
                    return Ok(Operand::Value(bool_dst));
                }
                if callee == "map_has" || callee == "map_remove" {
                    let bool_dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst: bool_dst,
                        kind: CastKind::Truncate,
                        from: IrType::I64,
                        to: IrType::Bool,
                        operand: Operand::Value(dst),
                    });
                    return Ok(Operand::Value(bool_dst));
                }
                return Ok(Operand::Value(dst));
            }

            if callee.starts_with("json_") {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "json_parse" => ("nex_rt_json_parse", IrType::I64),
                    "json_type" => ("nex_rt_json_type", IrType::String),
                    "json_len" => ("nex_rt_json_len", IrType::I64),
                    "json_at" => ("nex_rt_json_at", IrType::I64),
                    "json_get" => ("nex_rt_json_get", IrType::I64),
                    "json_str" => ("nex_rt_json_str", IrType::String),
                    "json_num" => ("nex_rt_json_num", IrType::F64),
                    "json_bool" => ("nex_rt_json_bool", IrType::I64),
                    _ => ("nex_rt_json_is_null", IrType::I64),
                };
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                if callee == "json_bool" || callee == "json_is_null" {
                    let dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst,
                        kind: CastKind::Truncate,
                        from: IrType::I64,
                        to: IrType::Bool,
                        operand: Operand::Value(raw),
                    });
                    return Ok(Operand::Value(dst));
                }
                return Ok(Operand::Value(raw));
            }

            if callee == "push" {
                if args.len() != 2 {
                    return Err("`push` expects 2 arguments".to_string());
                }
                let array_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                let value_ty = analysis.expr_type(args[1].span).unwrap_or(Type::Error);
                let element = match array_ty {
                    Type::DynArray(display) => analysis
                        .array_info(display)
                        .map(|info| info.elem)
                        .unwrap_or(Type::Error),
                    _ => Type::Error,
                };
                if element == Type::Error {
                    return Err("`push` expects a growable array".to_string());
                }
                if element != value_ty {
                    return Err(format!("`push` expects `{element}`, found `{value_ty}`"));
                }
                let elem_ir = IrType::from_type(element);
                let header_operand =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let header = operand_value_id(builder, header_operand)?;
                let len_slot = field_ptr(builder, header, 1, IrType::I64);
                let current_len = builder.new_value();
                builder.emit(Inst::Load {
                    dst: current_len,
                    ptr: len_slot,
                    ty: IrType::I64,
                });
                let needed = builder.new_value();
                builder.emit(Inst::Bin {
                    dst: needed,
                    op: BinOp::Add,
                    ty: IrType::I64,
                    lhs: Operand::Value(current_len),
                    rhs: Operand::Const(Const::Int(1)),
                });
                let ensured = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(ensured),
                    name: "nex_rt_array_ensure".to_string(),
                    args: vec![
                        Operand::Value(header),
                        Operand::Value(needed),
                        Operand::Const(Const::Int(ir_elem_size(elem_ir))),
                    ],
                    ret: IrType::String,
                });
                let data_slot = field_ptr(builder, ensured, 0, IrType::String);
                let data = builder.new_value();
                builder.emit(Inst::Load {
                    dst: data,
                    ptr: data_slot,
                    ty: IrType::String,
                });
                let slot = builder.new_value();
                builder.emit(Inst::DynIndexPtr {
                    dst: slot,
                    base: data,
                    index: Operand::Value(current_len),
                    elem_ty: elem_ir,
                });
                let value = lower_expr(builder, &args[1], scopes, storage, analysis, element)?;
                builder.emit(Inst::Store {
                    ptr: slot,
                    value,
                    ty: elem_ir,
                });
                let new_len_slot = field_ptr(builder, ensured, 1, IrType::I64);
                builder.emit(Inst::Store {
                    ptr: new_len_slot,
                    value: Operand::Value(needed),
                    ty: IrType::I64,
                });
                return Ok(Operand::Const(Const::Bool(true)));
            }

            if callee == "abs" {
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                let (name, ret) = if arg_ty == Type::F64 {
                    ("nex_rt_abs_f64", IrType::F64)
                } else {
                    ("nex_rt_abs", IrType::I64)
                };
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![arg_value],
                    ret,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "min" || callee == "max" {
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                let runtime_name = if arg_ty == Type::F64 {
                    if callee == "min" {
                        "nex_rt_min_f64"
                    } else {
                        "nex_rt_max_f64"
                    }
                } else {
                    if callee == "min" {
                        "nex_rt_min_i64"
                    } else {
                        "nex_rt_max_i64"
                    }
                };
                let ret_ty = if arg_ty == Type::F64 {
                    IrType::F64
                } else {
                    IrType::I64
                };
                let left = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let right = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: runtime_name.to_string(),
                    args: vec![left, right],
                    ret: ret_ty,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "sqrt" {
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_sqrt".to_string(),
                    args: vec![arg_value],
                    ret: IrType::F64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "pow" {
                let left = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let right = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_pow".to_string(),
                    args: vec![left, right],
                    ret: IrType::F64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "assert" {
                let cond = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let ok_bb = builder.new_block();
                let panic_bb = builder.new_block();
                builder.terminate(Terminator::CondBr {
                    cond,
                    then_bb: ok_bb,
                    else_bb: panic_bb,
                });
                builder.current = panic_bb;
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_panic".to_string(),
                    args: vec![Operand::Const(Const::Str("assertion failed".to_string()))],
                    ret: IrType::Void,
                });
                builder.terminate(Terminator::Unreachable);
                builder.current = ok_bb;
                return Ok(Operand::Const(Const::Bool(false)));
            }

            if callee == "panic" {
                let message =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_panic".to_string(),
                    args: vec![message],
                    ret: IrType::Void,
                });
                builder.terminate(Terminator::Unreachable);
                return Ok(Operand::Const(Const::Bool(false)));
            }

            if callee == "alloc" {
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_alloc".to_string(),
                    args: vec![arg_value],
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "free" {
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_free".to_string(),
                    args: vec![arg_value],
                    ret: IrType::Void,
                });
                return Ok(Operand::Const(Const::Bool(false)));
            }

            if callee == "peek" {
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_peek_i64".to_string(),
                    args: vec![arg_value],
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "poke" {
                let ptr = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let value = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                builder.emit(Inst::Call {
                    dst: None,
                    name: "nex_rt_poke_i64".to_string(),
                    args: vec![ptr, value],
                    ret: IrType::Void,
                });
                return Ok(Operand::Const(Const::Bool(false)));
            }

            if callee == "concat" {
                let left = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let right = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_str_concat".to_string(),
                    args: vec![left, right],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_contains" || callee == "str_index_of" {
                let haystack =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let needle = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                let (name, ret) = if callee == "str_contains" {
                    ("nex_rt_str_contains", IrType::I32)
                } else {
                    ("nex_rt_str_index_of", IrType::I64)
                };
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![haystack, needle],
                    ret,
                });
                if callee == "str_contains" {
                    // The runtime returns a C int; narrow it to the NEX bool type.
                    let bool_dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst: bool_dst,
                        kind: CastKind::Truncate,
                        from: IrType::I32,
                        to: IrType::Bool,
                        operand: Operand::Value(dst),
                    });
                    return Ok(Operand::Value(bool_dst));
                }
                return Ok(Operand::Value(dst));
            }

            if callee == "str_substring" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let start = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let end = lower_expr(builder, &args[2], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_str_substring".to_string(),
                    args: vec![value, start, end],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_upper" || callee == "str_lower" || callee == "str_trim" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                let name = match callee {
                    "str_upper" => "nex_rt_str_upper",
                    "str_lower" => "nex_rt_str_lower",
                    _ => "nex_rt_str_trim",
                };
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![value],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_index_of_from" || callee == "char_at" || callee == "char_code" {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "str_index_of_from" => ("nex_rt_str_index_of_from", IrType::I64),
                    "char_at" => ("nex_rt_char_at", IrType::String),
                    _ => ("nex_rt_char_code", IrType::I64),
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "unix_time"
                || callee == "now_ms"
                || callee == "http_date"
                || callee == "sleep_ms"
            {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret) = match callee {
                    "unix_time" => ("nex_rt_unix_time", IrType::I64),
                    "now_ms" => ("nex_rt_now_ms", IrType::I64),
                    "sleep_ms" => ("nex_rt_sleep_ms", IrType::Void),
                    _ => ("nex_rt_http_date", IrType::String),
                };
                let dst = if ret == IrType::Void {
                    None
                } else {
                    Some(builder.new_value())
                };
                builder.emit(Inst::Call {
                    dst,
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                return match dst {
                    Some(value) => Ok(Operand::Value(value)),
                    None => Ok(Operand::Const(Const::Bool(true))),
                };
            }

            if callee == "http_method"
                || callee == "http_path"
                || callee == "http_version"
                || callee == "http_header"
                || callee == "http_body"
                || callee == "http_response"
                || callee == "http_response_keepalive"
            {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let name = match callee {
                    "http_method" => "nex_rt_http_method",
                    "http_path" => "nex_rt_http_path",
                    "http_version" => "nex_rt_http_version",
                    "http_header" => "nex_rt_http_header",
                    "http_body" => "nex_rt_http_body",
                    "http_response_keepalive" => "nex_rt_http_response_keepalive",
                    _ => "nex_rt_http_response",
                };
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: call_args,
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "tcp_connect"
                || callee == "tcp_listen"
                || callee == "tcp_accept"
                || callee == "tcp_close"
                || callee == "tcp_write"
                || callee == "tcp_read"
            {
                let mut call_args = Vec::new();
                for arg in args {
                    call_args.push(lower_expr(
                        builder,
                        arg,
                        scopes,
                        storage,
                        analysis,
                        Type::Error,
                    )?);
                }
                let (name, ret, cast_to_bool) = match callee {
                    "tcp_connect" => ("nex_rt_tcp_connect", IrType::I64, false),
                    "tcp_listen" => ("nex_rt_tcp_listen", IrType::I64, false),
                    "tcp_accept" => ("nex_rt_tcp_accept", IrType::I64, false),
                    "tcp_write" => ("nex_rt_tcp_write", IrType::I64, false),
                    "tcp_read" => ("nex_rt_tcp_read", IrType::String, false),
                    _ => ("nex_rt_tcp_close", IrType::I32, true),
                };
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: name.to_string(),
                    args: call_args,
                    ret,
                });
                if cast_to_bool {
                    let dst = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst,
                        kind: CastKind::Truncate,
                        from: IrType::I32,
                        to: IrType::Bool,
                        operand: Operand::Value(raw),
                    });
                    return Ok(Operand::Value(dst));
                }
                return Ok(Operand::Value(raw));
            }

            if callee == "read_file" || callee == "file_exists" || callee == "file_size" {
                let path = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                let (name, ret) = match callee {
                    "read_file" => ("nex_rt_read_file", IrType::String),
                    "file_size" => ("nex_rt_file_size", IrType::I64),
                    _ => ("nex_rt_file_exists", IrType::I32),
                };
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![path],
                    ret,
                });
                if callee == "file_exists" {
                    let wide = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst: wide,
                        kind: CastKind::Truncate,
                        from: IrType::I32,
                        to: IrType::Bool,
                        operand: Operand::Value(dst),
                    });
                    return Ok(Operand::Value(wide));
                }
                return Ok(Operand::Value(dst));
            }

            if callee == "write_file" {
                let path = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let content =
                    lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: "nex_rt_write_file".to_string(),
                    args: vec![path, content],
                    ret: IrType::I32,
                });
                let dst = builder.new_value();
                builder.emit(Inst::Cast {
                    dst,
                    kind: CastKind::Truncate,
                    from: IrType::I32,
                    to: IrType::Bool,
                    operand: Operand::Value(raw),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "tcp_set_timeout" {
                let socket = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let timeout =
                    lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: "nex_rt_tcp_set_timeout".to_string(),
                    args: vec![socket, timeout],
                    ret: IrType::I32,
                });
                let dst = builder.new_value();
                builder.emit(Inst::Cast {
                    dst,
                    kind: CastKind::Truncate,
                    from: IrType::I32,
                    to: IrType::Bool,
                    operand: Operand::Value(raw),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_reverse" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_str_reverse".to_string(),
                    args: vec![value],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "parse_i64" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_parse_i64".to_string(),
                    args: vec![value],
                    ret: IrType::I64,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_starts_with" || callee == "str_ends_with" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let other = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let name = if callee == "str_starts_with" {
                    "nex_rt_str_starts_with"
                } else {
                    "nex_rt_str_ends_with"
                };
                let raw = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(raw),
                    name: name.to_string(),
                    args: vec![value, other],
                    ret: IrType::I32,
                });
                let dst = builder.new_value();
                builder.emit(Inst::Cast {
                    dst,
                    kind: CastKind::Truncate,
                    from: IrType::I32,
                    to: IrType::Bool,
                    operand: Operand::Value(raw),
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_repeat" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let count = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_str_repeat".to_string(),
                    args: vec![value, count],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "str_replace" {
                let value = lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let from = lower_expr(builder, &args[1], scopes, storage, analysis, Type::Error)?;
                let to = lower_expr(builder, &args[2], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: "nex_rt_str_replace".to_string(),
                    args: vec![value, from, to],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            if callee == "to_string" {
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
                let arg_value =
                    lower_expr(builder, &args[0], scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                if arg_ty == Type::Bool {
                    // The runtime takes a C int; widen the NEX bool first.
                    let wide = builder.new_value();
                    builder.emit(Inst::Cast {
                        dst: wide,
                        kind: CastKind::ZeroExtend,
                        from: IrType::Bool,
                        to: IrType::I32,
                        operand: arg_value,
                    });
                    builder.emit(Inst::Call {
                        dst: Some(dst),
                        name: "nex_rt_bool_to_string".to_string(),
                        args: vec![Operand::Value(wide)],
                        ret: IrType::String,
                    });
                    return Ok(Operand::Value(dst));
                }
                let name = match arg_ty {
                    Type::F64 => "nex_rt_f64_to_string",
                    _ => "nex_rt_i64_to_string",
                };
                builder.emit(Inst::Call {
                    dst: Some(dst),
                    name: name.to_string(),
                    args: vec![arg_value],
                    ret: IrType::String,
                });
                return Ok(Operand::Value(dst));
            }

            let signature = analysis
                .function(callee)
                .ok_or_else(|| format!("unknown function `{callee}` during code generation"))?;
            let ret = IrType::from_type(signature.ret);

            let mut arg_values = Vec::with_capacity(args.len());
            for arg in args {
                let value = lower_expr(builder, arg, scopes, storage, analysis, Type::Error)?;
                arg_values.push(value);
            }

            let dst = if ret == IrType::Void {
                None
            } else {
                Some(builder.new_value())
            };
            builder.emit(Inst::Call {
                dst,
                name: callee.to_string(),
                args: arg_values,
                ret,
            });

            if let Some(dst) = dst {
                Ok(Operand::Value(dst))
            } else {
                Ok(Operand::Const(Const::Bool(false)))
            }
        }
        ExprKind::Cast { expr: inner, ty: _ } => {
            let source_ty = analysis.expr_type(inner.span).unwrap_or(Type::Error);
            let target_ty = expr_ty;
            if source_ty == target_ty {
                return lower_expr(builder, inner, scopes, storage, analysis, Type::Error);
            }

            if target_ty == Type::Bool && (source_ty.is_integer() || source_ty == Type::Char) {
                let value = lower_expr(builder, inner, scopes, storage, analysis, Type::Error)?;
                let dst = builder.new_value();
                builder.emit(Inst::Bin {
                    dst,
                    op: BinOp::Ne,
                    ty: IrType::from_type(source_ty),
                    lhs: value,
                    rhs: Operand::Const(Const::Int(0)),
                });
                return Ok(Operand::Value(dst));
            }

            let kind = cast_kind(source_ty, target_ty)?;
            let value = lower_expr(builder, inner, scopes, storage, analysis, Type::Error)?;
            let dst = builder.new_value();
            builder.emit(Inst::Cast {
                dst,
                kind,
                from: IrType::from_type(source_ty),
                to: IrType::from_type(target_ty),
                operand: value,
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Field { base, name, .. } => {
            if let Some((_, index)) = analysis.variant_value(expr.span) {
                return Ok(Operand::Const(Const::Int(index as i64)));
            }
            // Module item access: `math.PI`.
            if let ExprKind::Ident(module_name) = &base.kind {
                if analysis.modules.contains(module_name) {
                    let qualified = format!("{}.{}", module_name, name);
                    if let Some(init) = analysis.const_exprs.get(&qualified) {
                        return lower_expr(builder, init, scopes, storage, analysis, Type::Error);
                    }
                }
            }
            let Some((struct_name, index, field_ty)) = analysis.field_read(expr.span) else {
                return Err("missing field information during code generation".to_string());
            };
            let field_ty = IrType::from_type(field_ty);
            let base_ptr = lower_struct_place(builder, base, scopes, storage, analysis)?;
            let field_ptr = builder.new_value();
            builder.emit(Inst::FieldPtr {
                dst: field_ptr,
                base: base_ptr,
                struct_name,
                index,
                field_ty,
            });
            let dst = builder.new_value();
            builder.emit(Inst::Load {
                dst,
                ptr: field_ptr,
                ty: field_ty,
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Array(elements) => {
            let source = lower_array_place(builder, expr, scopes, storage, analysis)?;
            let _ = elements;
            Ok(Operand::Value(source))
        }
        ExprKind::Index { base, index } => {
            let base_ty = analysis.expr_type(base.span).unwrap_or(Type::Error);
            if let Type::DynArray(_) = base_ty {
                return lower_dyn_index(builder, base, index, expr, scopes, storage, analysis);
            }
            let elem_ty = analysis.expr_type(expr.span).unwrap_or(Type::Error);
            let elem_ir = IrType::from_type(elem_ty);
            let base_ptr = lower_array_place(builder, base, scopes, storage, analysis)?;
            // base_ptr may come from struct place for arrays stored in struct fields (not supported); fallback above rarely used
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
                elem_ty: elem_ir,
            });
            let loaded = builder.new_value();
            builder.emit(Inst::Load {
                dst: loaded,
                ptr: dst,
                ty: elem_ir,
            });
            Ok(Operand::Value(loaded))
        }
        ExprKind::Tuple(_) => {
            let ty = analysis
                .expr_type(expr.span)
                .ok_or_else(|| "missing type information for tuple".to_string())?;
            let Type::Struct(name) = ty else {
                return Err("not a tuple value".to_string());
            };
            let source = lower_struct_place(builder, expr, scopes, storage, analysis)?;
            let dst = builder.new_value();
            builder.emit(Inst::Load {
                dst,
                ptr: source,
                ty: IrType::Struct(name),
            });
            Ok(Operand::Value(dst))
        }
        ExprKind::Error => Err("invalid expression reached code generation".to_string()),
    }
}

fn lower_and(
    builder: &mut FunctionBuilder,
    left: &Expr,
    right: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<Operand, String> {
    let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
    let temp = builder.alloca_at_entry(IrType::Bool);
    builder.emit(Inst::Store {
        ptr: temp,
        value: Operand::Const(Const::Bool(false)),
        ty: IrType::Bool,
    });

    let rhs_bb = builder.new_block();
    let end_bb = builder.new_block();
    builder.terminate(Terminator::CondBr {
        cond: lhs,
        then_bb: rhs_bb,
        else_bb: end_bb,
    });

    builder.current = rhs_bb;
    let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
    builder.emit(Inst::Store {
        ptr: temp,
        value: rhs,
        ty: IrType::Bool,
    });
    builder.terminate(Terminator::Br(end_bb));

    builder.current = end_bb;
    let dst = builder.new_value();
    builder.emit(Inst::Load {
        dst,
        ptr: temp,
        ty: IrType::Bool,
    });
    Ok(Operand::Value(dst))
}

fn lower_or(
    builder: &mut FunctionBuilder,
    left: &Expr,
    right: &Expr,
    scopes: &mut Vec<HashMap<String, Slot>>,
    storage: &HashMap<nex_diagnostics::Span, (ValueId, IrType)>,
    analysis: &Analysis,
) -> Result<Operand, String> {
    let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
    let temp = builder.alloca_at_entry(IrType::Bool);
    builder.emit(Inst::Store {
        ptr: temp,
        value: Operand::Const(Const::Bool(true)),
        ty: IrType::Bool,
    });

    let rhs_bb = builder.new_block();
    let end_bb = builder.new_block();
    builder.terminate(Terminator::CondBr {
        cond: lhs,
        then_bb: end_bb,
        else_bb: rhs_bb,
    });

    builder.current = rhs_bb;
    let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
    builder.emit(Inst::Store {
        ptr: temp,
        value: rhs,
        ty: IrType::Bool,
    });
    builder.terminate(Terminator::Br(end_bb));

    builder.current = end_bb;
    let dst = builder.new_value();
    builder.emit(Inst::Load {
        dst,
        ptr: temp,
        ty: IrType::Bool,
    });
    Ok(Operand::Value(dst))
}

fn lookup_slot(scopes: &[HashMap<String, Slot>], name: &str) -> Option<Slot> {
    for scope in scopes.iter().rev() {
        if let Some(slot) = scope.get(name) {
            return Some(*slot);
        }
    }
    None
}

type PrintCast = Option<(CastKind, IrType, IrType)>;

fn runtime_print_name(base: &str, ty: Type) -> Result<(String, PrintCast), String> {
    let suffix = if base == "println" { "ln" } else { "" };
    let name_prefix = if suffix.is_empty() {
        "nex_rt_print".to_string()
    } else {
        "nex_rt_println".to_string()
    };

    let (kind, cast) = match ty {
        Type::Bool => (
            "bool",
            Some((CastKind::ZeroExtend, IrType::Bool, IrType::I32)),
        ),
        Type::Char => ("char", None),
        Type::I8 => ("i64", Some((CastKind::SignExtend, IrType::I8, IrType::I64))),
        Type::I16 => (
            "i64",
            Some((CastKind::SignExtend, IrType::I16, IrType::I64)),
        ),
        Type::I32 => (
            "i64",
            Some((CastKind::SignExtend, IrType::I32, IrType::I64)),
        ),
        Type::I64 => ("i64", None),
        Type::U8 => ("u64", Some((CastKind::ZeroExtend, IrType::U8, IrType::I64))),
        Type::U16 => (
            "u64",
            Some((CastKind::ZeroExtend, IrType::U16, IrType::I64)),
        ),
        Type::U32 => (
            "u64",
            Some((CastKind::ZeroExtend, IrType::U32, IrType::I64)),
        ),
        Type::U64 => ("u64", None),
        Type::F32 => (
            "f64",
            Some((CastKind::FloatExtend, IrType::F32, IrType::F64)),
        ),
        Type::F64 => ("f64", None),
        Type::String => ("str", None),
        _ => return Err(format!("`{base}` cannot print a value of type `{ty}`")),
    };

    let runtime_name = if kind == "str" {
        if suffix.is_empty() {
            "nex_rt_print_str".to_string()
        } else {
            "nex_rt_println_str".to_string()
        }
    } else {
        format!("{name_prefix}_{kind}")
    };

    let _ = name_prefix;
    Ok((runtime_name, cast))
}
fn cast_kind(source: Type, target: Type) -> Result<CastKind, String> {
    use nex_semantic::Type as T;
    match (source, target) {
        (s, t) if s.is_integer() && t.is_integer() => {
            let s_bits = integer_bits(s);
            let t_bits = integer_bits(t);
            if t_bits > s_bits {
                if s.is_signed_integer() {
                    Ok(CastKind::SignExtend)
                } else {
                    Ok(CastKind::ZeroExtend)
                }
            } else {
                Ok(CastKind::Truncate)
            }
        }
        (s, t) if s.is_integer() && t.is_float() => {
            if s.is_unsigned_integer() {
                Ok(CastKind::UnsignedIntToFloat)
            } else {
                Ok(CastKind::SignedIntToFloat)
            }
        }
        (s, t) if s.is_float() && t.is_integer() => {
            if t.is_unsigned_integer() || t == T::Bool || t == T::Char {
                Ok(CastKind::FloatToUnsignedInt)
            } else {
                Ok(CastKind::FloatToSignedInt)
            }
        }
        (s, t) if s.is_float() && t.is_float() => {
            if integer_bits_float(t) > integer_bits_float(s) {
                Ok(CastKind::FloatExtend)
            } else {
                Ok(CastKind::Truncate)
            }
        }
        (T::Bool, t) if t.is_integer() || t == T::Char => Ok(CastKind::ZeroExtend),
        (T::Char, t) if t.is_integer() => {
            if integer_bits(t) >= 32 {
                Ok(CastKind::ZeroExtend)
            } else {
                Ok(CastKind::Truncate)
            }
        }
        (s, T::Char) if s.is_integer() => {
            if integer_bits(s) <= 32 {
                Ok(CastKind::ZeroExtend)
            } else {
                Ok(CastKind::Truncate)
            }
        }
        _ => Err(format!("cannot lower cast from {source} to {target}")),
    }
}

fn integer_bits(ty: Type) -> u32 {
    match ty {
        Type::I8 | Type::U8 | Type::Bool => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 | Type::Char => 32,
        Type::I64 | Type::U64 => 64,
        _ => 0,
    }
}

fn integer_bits_float(ty: Type) -> u32 {
    match ty {
        Type::F32 => 32,
        Type::F64 => 64,
        _ => 0,
    }
}
