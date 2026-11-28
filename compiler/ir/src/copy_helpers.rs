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
    // Clone the field list so we can iterate while potentially recursing.
    let fields: Vec<(String, Type)> = info.fields.clone();
    for (index, (_, field_ty)) in fields.iter().enumerate() {
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
        if let Type::DynArray(display) = *field_ty {
            // Value semantics: clone the growable array instead of aliasing
            // the (move-only) header pointer.
            let src_value = builder.new_value();
            builder.emit(Inst::Load {
                dst: src_value,
                ptr: src_field,
                ty: IrType::String,
            });
            let info = analysis.array_info(display).cloned();
            let elem = info.as_ref().map(|i| i.elem).unwrap_or(Type::Error);
            if let Type::Struct(elem_struct) = elem {
                let cloned = clone_struct_array(builder, src_value, elem_struct, analysis)?;
                builder.emit(Inst::Store {
                    ptr: dst_field,
                    value: Operand::Value(cloned),
                    ty: IrType::String,
                });
            } else {
                let elem_size = info
                    .map(|i| ir_elem_size(analysis, IrType::from_type(i.elem)))
                    .unwrap_or(8);
                let cloned = builder.new_value();
                builder.emit(Inst::Call {
                    dst: Some(cloned),
                    name: "nex_rt_array_clone".to_string(),
                    args: vec![
                        Operand::Value(src_value),
                        Operand::Const(Const::Int(elem_size)),
                    ],
                    ret: IrType::String,
                });
                builder.emit(Inst::Store {
                    ptr: dst_field,
                    value: Operand::Value(cloned),
                    ty: IrType::String,
                });
            }
        } else if let Type::Struct(nested) = *field_ty {
            // Recurse so nested growable arrays are cloned too.
            copy_struct(builder, dst_field, src_field, nested, analysis)?;
        } else {
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
    }
    Ok(())
}

fn free_struct_arrays(
    builder: &mut FunctionBuilder,
    base: ValueId,
    struct_name: &'static str,
    analysis: &Analysis,
) {
    let Some(info) = analysis.struct_info(struct_name) else {
        return;
    };
    let fields: Vec<(String, Type)> = info.fields.clone();
    for (index, (_, field_ty)) in fields.iter().enumerate() {
        let ir_ty = IrType::from_type(*field_ty);
        let field_ptr = builder.new_value();
        builder.emit(Inst::FieldPtr {
            dst: field_ptr,
            base,
            struct_name,
            index: index as u32,
            field_ty: ir_ty,
        });
        if let Type::DynArray(_) = field_ty {
            let header = builder.new_value();
            builder.emit(Inst::Load {
                dst: header,
                ptr: field_ptr,
                ty: IrType::String,
            });
            builder.emit(Inst::Call {
                dst: None,
                name: "nex_rt_array_free".to_string(),
                args: vec![Operand::Value(header)],
                ret: IrType::Void,
            });
        } else if let Type::Struct(nested) = field_ty {
            free_struct_arrays(builder, field_ptr, nested, analysis);
        }
    }
}

fn reset_struct_arrays(
    builder: &mut FunctionBuilder,
    base: ValueId,
    struct_name: &'static str,
    analysis: &Analysis,
) {
    let Some(info) = analysis.struct_info(struct_name) else {
        return;
    };
    let fields: Vec<(String, Type)> = info.fields.clone();
    for (index, (_, field_ty)) in fields.iter().enumerate() {
        let ir_ty = IrType::from_type(*field_ty);
        let field_ptr = builder.new_value();
        builder.emit(Inst::FieldPtr {
            dst: field_ptr,
            base,
            struct_name,
            index: index as u32,
            field_ty: ir_ty,
        });
        if let Type::DynArray(_) = field_ty {
            let header = builder.new_value();
            builder.emit(Inst::Load {
                dst: header,
                ptr: field_ptr,
                ty: IrType::String,
            });
            builder.emit(Inst::Call {
                dst: None,
                name: "nex_rt_array_free".to_string(),
                args: vec![Operand::Value(header)],
                ret: IrType::Void,
            });
            let empty = builder.new_value();
            builder.emit(Inst::Call {
                dst: Some(empty),
                name: "nex_rt_array_new".to_string(),
                args: Vec::new(),
                ret: IrType::String,
            });
            builder.emit(Inst::Store {
                ptr: field_ptr,
                value: Operand::Value(empty),
                ty: IrType::String,
            });
        } else if let Type::Struct(nested) = field_ty {
            reset_struct_arrays(builder, field_ptr, nested, analysis);
        }
    }
}

fn clone_struct_array(
    builder: &mut FunctionBuilder,
    src_header: ValueId,
    struct_name: &'static str,
    analysis: &Analysis,
) -> Result<ValueId, String> {
    let elem_ir = IrType::Struct(struct_name);
    let elem_size = ir_elem_size(analysis, elem_ir);
    let len_slot = field_ptr(builder, src_header, 1, IrType::I64);
    let len_value = builder.new_value();
    builder.emit(Inst::Load {
        dst: len_value,
        ptr: len_slot,
        ty: IrType::I64,
    });
    let dst_header = builder.new_value();
    builder.emit(Inst::Call {
        dst: Some(dst_header),
        name: "nex_rt_array_new".to_string(),
        args: Vec::new(),
        ret: IrType::String,
    });
    let ensured = builder.new_value();
    builder.emit(Inst::Call {
        dst: Some(ensured),
        name: "nex_rt_array_ensure".to_string(),
        args: vec![
            Operand::Value(dst_header),
            Operand::Value(len_value),
            Operand::Const(Const::Int(elem_size)),
        ],
        ret: IrType::String,
    });
    let len_slot_dst = field_ptr(builder, ensured, 1, IrType::I64);
    builder.emit(Inst::Store {
        ptr: len_slot_dst,
        value: Operand::Value(len_value),
        ty: IrType::I64,
    });

    let idx_slot = builder.alloca_at_entry(IrType::I64);
    builder.emit(Inst::Store {
        ptr: idx_slot,
        value: Operand::Const(Const::Int(0)),
        ty: IrType::I64,
    });

    let cond_bb = builder.new_block();
    let body_bb = builder.new_block();
    let exit_bb = builder.new_block();
    builder.terminate(Terminator::Br(cond_bb));

    builder.current = cond_bb;
    let idx_val = builder.new_value();
    builder.emit(Inst::Load {
        dst: idx_val,
        ptr: idx_slot,
        ty: IrType::I64,
    });
    let cmp = builder.new_value();
    builder.emit(Inst::Bin {
        dst: cmp,
        op: BinOp::Lt,
        ty: IrType::I64,
        lhs: Operand::Value(idx_val),
        rhs: Operand::Value(len_value),
    });
    builder.terminate(Terminator::CondBr {
        cond: Operand::Value(cmp),
        then_bb: body_bb,
        else_bb: exit_bb,
    });

    builder.current = body_bb;
    let src_data_slot = field_ptr(builder, src_header, 0, IrType::String);
    let src_data = builder.new_value();
    builder.emit(Inst::Load {
        dst: src_data,
        ptr: src_data_slot,
        ty: IrType::String,
    });
    let dst_data_slot = field_ptr(builder, ensured, 0, IrType::String);
    let dst_data = builder.new_value();
    builder.emit(Inst::Load {
        dst: dst_data,
        ptr: dst_data_slot,
        ty: IrType::String,
    });
    let src_elem = builder.new_value();
    builder.emit(Inst::DynIndexPtr {
        dst: src_elem,
        base: src_data,
        index: Operand::Value(idx_val),
        elem_ty: elem_ir,
    });
    let dst_elem = builder.new_value();
    builder.emit(Inst::DynIndexPtr {
        dst: dst_elem,
        base: dst_data,
        index: Operand::Value(idx_val),
        elem_ty: elem_ir,
    });
    copy_struct(builder, dst_elem, src_elem, struct_name, analysis)?;
    let new_idx = builder.new_value();
    builder.emit(Inst::Bin {
        dst: new_idx,
        op: BinOp::Add,
        ty: IrType::I64,
        lhs: Operand::Value(idx_val),
        rhs: Operand::Const(Const::Int(1)),
    });
    builder.emit(Inst::Store {
        ptr: idx_slot,
        value: Operand::Value(new_idx),
        ty: IrType::I64,
    });
    builder.terminate(Terminator::Br(cond_bb));

    builder.current = exit_bb;
    Ok(ensured)
}

