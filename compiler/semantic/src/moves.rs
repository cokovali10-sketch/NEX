//! Move checker (ownership stage 1).
//!
//! NEX 1.0 has value (copy) semantics for every type. This pass introduces
//! *move semantics* for the heap-owning types (`string` and growable arrays
//! `[T]`): binding such a value to another variable moves ownership, and any
//! later use of the moved-from variable is a `NXS050` error.
//!
//! The checker is a conservative, straight-line analysis. It tracks moved
//! bindings per lexical scope. Moves inside a nested block (an `if`/`while`/
//! `for`/`match` body) do not escape that block, so the checker never reports a
//! false positive; the trade-off is that a use-after-move across a branch is
//! not caught yet (a documented stage-1 limitation).
//!
//! Only *locals* participate: parameters, loop variables, and module-level
//! globals are never treated as move sources in this stage, which keeps the
//! analysis free of false positives while the borrow checker (stage 2) is built.

use std::collections::HashMap;

use nex_ast::{Block, Expr, ExprKind, FnDecl, MatchArm, Program, Stmt, StmtKind};

use crate::{Analysis, Type};

#[derive(Debug, Clone, Copy)]
struct VarState {
    ty: Type,
    moved: bool,
}

pub(crate) fn check_moves(
    program: &Program,
    analysis: &Analysis,
) -> Vec<nex_diagnostics::Diagnostic> {
    let mut checker = MoveChecker {
        analysis,
        diagnostics: Vec::new(),
        scopes: Vec::new(),
    };

    for function in &program.functions {
        let param_types = analysis
            .function(&function.name)
            .map(|sig| sig.params.clone())
            .unwrap_or_default();
        checker.check_function(function, &param_types);
    }

    for imp in &program.impls {
        for method in &imp.methods {
            let qualified = format!("{}.{}", imp.type_name, method.name);
            let param_types = analysis
                .function(&qualified)
                .map(|sig| sig.params.clone())
                .unwrap_or_default();
            checker.check_function(method, &param_types);
        }
    }

    checker.diagnostics
}

struct MoveChecker<'a> {
    analysis: &'a Analysis,
    diagnostics: Vec<nex_diagnostics::Diagnostic>,
    /// Lexical scopes, innermost last. Each maps a local name to its state.
    scopes: Vec<HashMap<String, VarState>>,
}

impl<'a> MoveChecker<'a> {
    fn check_function(&mut self, function: &FnDecl, param_types: &[Type]) {
        self.scopes.push(HashMap::new());
        for (param, ty) in function.params.iter().zip(param_types.iter()) {
            // Parameters are bindings the caller owns; this stage never moves them.
            self.scopes.last_mut().unwrap().insert(
                param.name.clone(),
                VarState {
                    ty: *ty,
                    moved: false,
                },
            );
        }
        self.check_block(&function.body);
        self.scopes.pop();
    }

    fn check_block(&mut self, block: &Block) {
        self.scopes.push(HashMap::new());
        for statement in &block.stmts {
            self.check_statement(statement);
        }
        self.scopes.pop();
    }

    fn check_statement(&mut self, statement: &Stmt) {
        match &statement.kind {
            StmtKind::Let {
                name, names, init, ..
            } => {
                self.check_expr_reads(init);
                self.move_if_ident(init);
                // New bindings are live; record their declared type when known.
                let declared = self.analysis.decl_type(statement.span);
                if let Some(names) = names {
                    // Tuple destructuring: the individual element types are not
                    // tracked in this stage, so treat the bindings as Copy.
                    for binding in names {
                        self.bind_local(binding, Type::Error);
                    }
                } else {
                    self.bind_local(name, declared.unwrap_or(Type::Error));
                }
            }
            StmtKind::Assign { name, value } => {
                self.check_expr_reads(value);
                self.move_if_ident(value);
                // Assigning to a binding re-initialises it: it becomes live again.
                self.reinit(name);
            }
            StmtKind::AssignMany { names, values } => {
                for value in values {
                    self.check_expr_reads(value);
                }
                for value in values {
                    self.move_if_ident(value);
                }
                for name in names {
                    self.reinit(name);
                }
            }
            StmtKind::AssignField { base, value, .. } => {
                self.check_name_read(base, statement.span);
                self.check_expr_reads(value);
            }
            StmtKind::AssignIndex {
                base, index, value, ..
            } => {
                self.check_name_read(base, statement.span);
                self.check_expr_reads(index);
                self.check_expr_reads(value);
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    self.check_expr_reads(condition);
                    self.check_block(body);
                }
                if let Some(body) = else_branch {
                    self.check_block(body);
                }
            }
            StmtKind::While { cond, body } => {
                self.check_expr_reads(cond);
                self.check_block(body);
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                self.check_expr_reads(start);
                self.check_expr_reads(end);
                self.scopes.push(HashMap::new());
                // The loop variable is a fresh per-iteration binding; never a
                // move source in this stage.
                self.scopes.last_mut().unwrap().insert(
                    var.clone(),
                    VarState {
                        ty: Type::Error,
                        moved: false,
                    },
                );
                for statement in &body.stmts {
                    self.check_statement(statement);
                }
                self.scopes.pop();
            }
            StmtKind::Return(Some(value)) => self.check_expr_reads(value),
            StmtKind::Return(None) => {}
            StmtKind::Match { scrutinee, arms } => {
                self.check_expr_reads(scrutinee);
                for arm in arms {
                    self.check_arm(arm);
                }
            }
            StmtKind::Unsafe(block) | StmtKind::Defer(block) => self.check_block(block),
            StmtKind::Expr(expr) => self.check_expr_reads(expr),
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }

    fn check_arm(&mut self, arm: &MatchArm) {
        if let Some(guard) = &arm.guard {
            self.check_expr_reads(guard);
        }
        self.check_block(&arm.body);
    }

    /// Marks a variable moved when the right-hand side is a bare identifier of a
    /// move-only type. The read of that identifier already happened in
    /// `check_expr_reads`, so the move itself is not reported.
    fn move_if_ident(&mut self, expr: &Expr) {
        let ExprKind::Ident(name) = &expr.kind else {
            return;
        };
        let Some(state) = self.lookup(name) else {
            return;
        };
        if state.moved || !state.ty.is_move_only() {
            return;
        }
        self.mark_moved(name);
    }

    fn bind_local(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), VarState { ty, moved: false });
        }
    }

    /// Marks a binding live again after it is assigned to.
    fn reinit(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(state) = scope.get_mut(name) {
                state.moved = false;
                return;
            }
        }
    }

    /// Reports `NXS050` if the named local has been moved.
    fn check_name_read(&mut self, name: &str, span: nex_diagnostics::Span) {
        if let Some(state) = self.lookup(name) {
            if state.moved && state.ty.is_move_only() {
                self.diagnostics.push(nex_diagnostics::Diagnostic::error(
                    "NXS050",
                    format!("use of moved value `{name}`"),
                    span,
                ));
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<VarState> {
        for scope in self.scopes.iter().rev() {
            if let Some(state) = scope.get(name) {
                return Some(*state);
            }
        }
        None
    }

    /// Sets `moved = true` on the binding in the scope that owns it.
    fn mark_moved(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(state) = scope.get_mut(name) {
                state.moved = true;
                return;
            }
        }
    }

    /// Walks an expression in a read (borrow) position: every use of a moved
    /// local is an error, but nothing is marked moved.
    fn check_expr_reads(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some(state) = self.lookup(name) {
                    if state.moved && state.ty.is_move_only() {
                        self.diagnostics.push(nex_diagnostics::Diagnostic::error(
                            "NXS050",
                            format!("use of moved value `{name}`"),
                            expr.span,
                        ));
                    }
                }
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Str(_)
            | ExprKind::Char(_)
            | ExprKind::Bool(_)
            | ExprKind::Error => {}
            ExprKind::Unary { operand, .. } => self.check_expr_reads(operand),
            ExprKind::Binary { left, right, .. } => {
                self.check_expr_reads(left);
                self.check_expr_reads(right);
            }
            ExprKind::Call { args, .. } => {
                for arg in args {
                    self.check_expr_reads(arg);
                }
            }
            ExprKind::Cast { expr, .. } => self.check_expr_reads(expr),
            ExprKind::Tuple(items) | ExprKind::Array(items) => {
                for item in items {
                    self.check_expr_reads(item);
                }
            }
            ExprKind::Index { base, index } => {
                self.check_expr_reads(base);
                self.check_expr_reads(index);
            }
            ExprKind::Field { base, .. } => self.check_expr_reads(base),
            ExprKind::Try(inner) => self.check_expr_reads(inner),
        }
    }
}
