use std::collections::{HashMap, HashSet};

use nex_ast::{Block, Expr, ExprKind, Program, Stmt, StmtKind};
use nex_semantic::{Analysis, Type};

use crate::types::{
    BinOp, BlockId, CastKind, Const, Inst, IrFunction, IrModule, IrType, Operand, Terminator, UnOp,
    ValueId,
};

#[derive(Clone)]
enum Slot {
    Value(ValueId),
    Local(ValueId, IrType),
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
    let mut module = IrModule {
        functions: Vec::new(),
    };

    for function in &program.functions {
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
        for (param, (value, _ty)) in function.params.iter().zip(builder.func.params.iter()) {
            param_scope.insert(param.name.clone(), Slot::Value(*value));
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
            let value = lower_expr(builder, init, scopes, storage, analysis, expected)?;
            builder.emit(Inst::Store { ptr, value, ty });
            if let Some(scope) = scopes.last_mut() {
                scope.insert(name.clone(), Slot::Local(ptr, ty));
            }
            Ok(())
        }
        StmtKind::Assign { name, value } => {
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
            match slot {
                Slot::Local(ptr, ty) => {
                    let operand =
                        lower_expr(builder, value, scopes, storage, analysis, Type::Error)?;
                    // Type information is already checked; use the storage type.
                    let _ = ty;
                    builder.emit(Inst::Store {
                        ptr,
                        value: operand,
                        ty,
                    });
                    Ok(())
                }
                Slot::Value(_) => Err(format!("cannot assign to immutable binding `{name}`")),
            }
        }
        StmtKind::If {
            branches,
            else_branch,
        } => lower_if(builder, branches, else_branch, scopes, storage, analysis),
        StmtKind::While { cond, body } => {
            lower_while(builder, cond, body, scopes, storage, analysis)
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

    for (cond, body) in branches {
        let body_bb = builder.new_block();
        let next_bb = builder.new_block();
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
    Ok(())
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
            let slot = lookup_slot(scopes, name)
                .ok_or_else(|| format!("unknown variable `{name}` during code generation"))?;
            match slot {
                Slot::Value(value) => Ok(Operand::Value(value)),
                Slot::Local(ptr, ty) => {
                    let dst = builder.new_value();
                    builder.emit(Inst::Load { dst, ptr, ty });
                    Ok(Operand::Value(dst))
                }
            }
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
            let operand_ty =
                IrType::from_type(analysis.expr_type(left.span).unwrap_or(Type::Error));

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

            let lhs = lower_expr(builder, left, scopes, storage, analysis, Type::Error)?;
            let rhs = lower_expr(builder, right, scopes, storage, analysis, Type::Error)?;
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
            if callee == "print" || callee == "println" {
                if args.len() != 1 {
                    return Err(format!("`{callee}` expects exactly one argument"));
                }
                let arg_ty = analysis.expr_type(args[0].span).unwrap_or(Type::Error);
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
                name: callee.clone(),
                args: arg_values,
                ret,
            });

            if let Some(dst) = dst {
                Ok(Operand::Value(dst))
            } else {
                Ok(Operand::Const(Const::Bool(false)))
            }
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
            return Some(slot.clone());
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
