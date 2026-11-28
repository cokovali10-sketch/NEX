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
