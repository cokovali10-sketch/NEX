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

