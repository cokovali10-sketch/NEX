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

use std::collections::{HashMap, HashSet};

use nex_ast::{Block, Expr, ExprKind, FnDecl, Program, Stmt, StmtKind};

use crate::{Analysis, Type};

#[derive(Debug, Clone, Copy)]
struct VarState {
    ty: Type,
    mutable: bool,
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
        active_borrows: HashMap::new(),
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
    /// Borrows created while checking the current statement: name -> is_mut.
    /// Ephemeral: cleared at the start of every statement.
    active_borrows: HashMap<String, bool>,
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
                    mutable: false,
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
        self.active_borrows.clear();
        match &statement.kind {
            StmtKind::Let {
                name,
                names,
                mutable,
                init,
                ..
            } => {
                self.check_expr_reads(init);
                self.move_if_ident(init);
                if matches!(init.kind, ExprKind::Borrow { .. }) {
                    self.store_borrow_error(statement.span);
                }
                // New bindings are live; record their declared type when known.
                let declared = self.analysis.decl_type(statement.span);
                if let Some(names) = names {
                    // Tuple destructuring: the individual element types are not
                    // tracked in this stage, so treat the bindings as Copy.
                    for binding in names {
                        self.bind_local(binding, Type::Error, *mutable);
                    }
                } else {
                    self.bind_local(name, declared.unwrap_or(Type::Error), *mutable);
                }
            }
            StmtKind::Assign { name, value } => {
                self.check_expr_reads(value);
                self.move_if_ident(value);
                if matches!(value.kind, ExprKind::Borrow { .. }) {
                    self.store_borrow_error(statement.span);
                }
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
            StmtKind::AssignDeref { place, value } => {
                self.check_expr_reads(place);
                self.check_expr_reads(value);
            }
            StmtKind::AssignFieldDeref {
                reference, value, ..
            } => {
                self.check_expr_reads(reference);
                self.check_expr_reads(value);
            }
            StmtKind::AssignIndexDeref {
                reference,
                index,
                value,
            } => {
                self.check_expr_reads(reference);
                self.check_expr_reads(index);
                self.check_expr_reads(value);
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                // A move inside a branch must not unconditionally mark an outer
                // binding moved: the branch may not run. Only a *definite* move
                // (present on every path, including the else) makes the binding
                // moved after the `if`. This also avoids false positives when
                // there is no else.
                let saved = self.scopes.clone();
                let mut branch_moves: Vec<HashSet<String>> = Vec::new();
                for (condition, body) in branches {
                    self.scopes = saved.clone();
                    self.check_expr_reads(condition);
                    self.check_block(body);
                    branch_moves.push(self.moved_set());
                }
                if let Some(body) = else_branch {
                    self.scopes = saved.clone();
                    self.check_block(body);
                    branch_moves.push(self.moved_set());
                } else {
                    branch_moves.push(HashSet::new());
                }
                self.scopes = saved;
                let mut iter = branch_moves.into_iter();
                let mut definite = iter.next().unwrap_or_default();
                for set in iter {
                    definite.retain(|name| set.contains(name));
                }
                for name in definite {
                    self.mark_moved(&name);
                }
            }
            StmtKind::While { cond, body } => {
                self.check_expr_reads(cond);
                // A loop body may run zero times: discard moves to outer bindings.
                let saved = self.scopes.clone();
                self.check_block(body);
                self.scopes = saved;
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                self.check_expr_reads(start);
                self.check_expr_reads(end);
                // The loop may run zero times and `var` is a fresh per-iteration
                // binding; isolate the body and discard any moves to outer bindings.
                let saved = self.scopes.clone();
                self.scopes.push(HashMap::new());
                self.scopes.last_mut().unwrap().insert(
                    var.clone(),
                    VarState {
                        ty: Type::Error,
                        mutable: false,
                        moved: false,
                    },
                );
                for statement in &body.stmts {
                    self.check_statement(statement);
                }
                self.scopes = saved;
            }
            StmtKind::Return(Some(value)) => {
                self.check_expr_reads(value);
                self.check_return_borrow(value);
            }
            StmtKind::Return(None) => {}
            StmtKind::Match { scrutinee, arms } => {
                self.check_expr_reads(scrutinee);
                // At most one arm runs, and maybe none: analyse each arm from the
                // same pre-match state and discard moves to outer bindings.
                let saved = self.scopes.clone();
                for arm in arms {
                    self.scopes = saved.clone();
                    if let Some(guard) = &arm.guard {
                        self.check_expr_reads(guard);
                    }
                    self.check_block(&arm.body);
                }
                self.scopes = saved;
            }
            StmtKind::Unsafe(block) | StmtKind::Defer(block) => self.check_block(block),
            StmtKind::Expr(expr) => self.check_expr_reads(expr),
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }

    /// Names of every visible binding currently marked moved.
    fn moved_set(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for scope in &self.scopes {
            for (name, state) in scope {
                if state.moved {
                    set.insert(name.clone());
                }
            }
        }
        set
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

    fn bind_local(&mut self, name: &str, ty: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.to_string(),
                VarState {
                    ty,
                    mutable,
                    moved: false,
                },
            );
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

    /// Rejects storing a reference in a variable. Without lifetimes, a stored
    /// reference could outlive its referent, so (for now) references must be
    /// passed directly to a function call.
    fn store_borrow_error(&mut self, span: nex_diagnostics::Span) {
        self.diagnostics.push(nex_diagnostics::Diagnostic::error(
            "NXS056",
            "cannot store a reference in a variable; lifetimes are not implemented yet — pass it directly to a function",
            span,
        ));
    }

    /// Rejects `return &local` / `return &mut local`: the returned reference
    /// would point into the callee's frame and dangle after return.
    fn check_return_borrow(&mut self, expr: &Expr) {
        if let ExprKind::Borrow { place, .. } = &expr.kind {
            if let ExprKind::Ident(name) = &place.kind {
                if self.lookup(name).is_some() {
                    self.diagnostics.push(nex_diagnostics::Diagnostic::error(
                        "NXS055",
                        format!("cannot return a reference to local `{name}`; it would dangle"),
                        expr.span,
                    ));
                }
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
            ExprKind::Call { callee, args, .. } => {
                if callee == "drop" {
                    // `drop(x)` consumes `x`: flag a prior move, then mark it moved.
                    for arg in args {
                        self.check_expr_reads(arg);
                        self.move_if_ident(arg);
                    }
                } else {
                    for arg in args {
                        self.check_expr_reads(arg);
                    }
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
            ExprKind::Borrow { mutable, place } => {
                if let ExprKind::Ident(name) = &place.kind {
                    if *mutable {
                        if let Some(state) = self.lookup(name) {
                            if !state.mutable {
                                self.diagnostics.push(nex_diagnostics::Diagnostic::error(
                                    "NXS054",
                                    format!(
                                        "cannot mutably borrow `{name}`: declare it with `var` first"
                                    ),
                                    place.span,
                                ));
                            }
                        }
                    }
                    if let Some(existing_mut) = self.active_borrows.get(name).copied() {
                        let conflict = *mutable || existing_mut;
                        if conflict {
                            let message = if *mutable {
                                format!(
                                    "cannot mutably borrow `{name}`: it is already borrowed in this statement"
                                )
                            } else {
                                format!(
                                    "cannot borrow `{name}`: it is already mutably borrowed in this statement"
                                )
                            };
                            self.diagnostics.push(nex_diagnostics::Diagnostic::error(
                                "NXS053", message, place.span,
                            ));
                        }
                    } else {
                        self.active_borrows.insert(name.clone(), *mutable);
                    }
                }
                self.check_expr_reads(place);
            }
            ExprKind::Deref(inner) => self.check_expr_reads(inner),
        }
    }
}
