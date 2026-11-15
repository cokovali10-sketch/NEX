//! Static checks for NEX sources.
//!
//! The linter works on the parsed AST, so it only reports on files that already
//! compile. Every check is written to avoid false positives: it stays silent
//! when the information is not available rather than guessing.

use nex_ast::{Block, Expr, ExprKind, Program, Stmt, StmtKind};
use nex_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

/// A single linter finding.
pub struct Warning {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

/// Result of linting one source file.
pub struct LintResult {
    pub warnings: Vec<Warning>,
    /// Lexer or parser diagnostics. Non-empty means the file does not compile,
    /// so linting was skipped.
    pub diagnostics: Vec<Diagnostic>,
}

impl LintResult {
    pub fn is_empty(&self) -> bool {
        self.warnings.is_empty() && self.diagnostics.is_empty()
    }
}

/// Lints `source`.
pub fn lint_source(source: &str) -> LintResult {
    let lexed = nex_lexer::Lexer::new(source).lex();
    if !lexed.diagnostics.is_empty() {
        return LintResult {
            warnings: Vec::new(),
            diagnostics: lexed.diagnostics,
        };
    }

    let parsed = nex_parser::parse(&lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        return LintResult {
            warnings: Vec::new(),
            diagnostics: parsed.diagnostics,
        };
    }

    LintResult {
        warnings: lint_program(&parsed.program),
        diagnostics: Vec::new(),
    }
}

fn lint_program(program: &Program) -> Vec<Warning> {
    let mut warnings = Vec::new();

    // Collect every name that is used anywhere in the file.
    let mut used_idents = HashSet::new();
    let mut called_functions = HashSet::new();
    for function in &program.functions {
        collect_block_idents(&function.body, &mut used_idents);
        collect_block_calls(&function.body, &mut called_functions);
    }
    for constant in &program.consts {
        collect_expr_idents(&constant.init, &mut used_idents);
        collect_expr_calls(&constant.init, &mut called_functions);
    }

    // Unused private functions.
    for function in &program.functions {
        let is_entry_point = function.name == "main";
        let is_extern = function.extern_abi.is_some();
        if function.public || is_entry_point || is_extern {
            continue;
        }
        if !called_functions.contains(&function.name) {
            warnings.push(Warning {
                code: "NXL002",
                message: format!("function `{}` is never called", function.name),
                span: function.span,
            });
        }
    }

    // Unused imports.
    for item in &program.uses {
        let Some(last) = item.path.last() else {
            continue;
        };
        if !used_idents.contains(last) {
            warnings.push(Warning {
                code: "NXL005",
                message: format!("`{}` is imported but never used", item.path.join(".")),
                span: item.span,
            });
        }
    }

    // Per-function checks.
    for function in &program.functions {
        check_block(&function.body, &mut warnings);
    }

    warnings.sort_by_key(|warning| warning.span.start);
    warnings
}

/// Checks bindings, constant conditions, and self-comparisons inside a block.
fn check_block(block: &Block, warnings: &mut Vec<Warning>) {
    // Bindings that are never used later in the same block.
    let mut used = HashSet::new();
    collect_block_idents_excluding(block, &mut used);
    for stmt in &block.stmts {
        if let StmtKind::Let { name, ty, .. } = &stmt.kind {
            let reported =
                name.starts_with('_') || used.contains(name) || type_mentions(ty.as_ref(), &used);
            if !reported {
                warnings.push(Warning {
                    code: "NXL001",
                    message: format!("`{name}` is declared but never used"),
                    span: stmt.span,
                });
            }
        }
    }

    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::If { branches, .. } => {
                for (cond, body) in branches {
                    if let ExprKind::Bool(_) = cond.kind {
                        warnings.push(Warning {
                            code: "NXL004",
                            message: "condition is a constant".to_string(),
                            span: cond.span,
                        });
                    }
                    check_block(body, warnings);
                }
            }
            StmtKind::While { cond, body } => {
                if let ExprKind::Bool(_) = cond.kind {
                    warnings.push(Warning {
                        code: "NXL004",
                        message: "condition is a constant".to_string(),
                        span: cond.span,
                    });
                }
                check_block(body, warnings);
            }
            StmtKind::For { body, .. } => check_block(body, warnings),
            StmtKind::Match { arms, .. } => {
                for arm in arms {
                    check_block(&arm.body, warnings);
                }
            }
            StmtKind::Expr(expr) => check_expr(expr, warnings),
            _ => {}
        }
    }
}

fn check_expr(expr: &Expr, warnings: &mut Vec<Warning>) {
    if let ExprKind::Binary { op, left, right } = &expr.kind {
        let left_text = expr_text(left);
        let same = matches!(op, nex_ast::BinOp::Eq | nex_ast::BinOp::Ne)
            && !left_text.is_empty()
            && left_text == expr_text(right);
        if same {
            warnings.push(Warning {
                code: "NXL003",
                message: "comparison of an expression with itself".to_string(),
                span: expr.span,
            });
        }
        check_expr(left, warnings);
        check_expr(right, warnings);
        return;
    }
    if let ExprKind::Call { args, .. } = &expr.kind {
        for arg in args {
            check_expr(arg, warnings);
        }
    }
}

fn expr_text(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Ident(name) => name.clone(),
        ExprKind::Int(value) => value.to_string(),
        ExprKind::Str(value) => value.clone(),
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::Field { base, name, .. } => format!("{}.{}", expr_text(base), name),
        _ => String::new(),
    }
}

fn type_mentions(ty: Option<&nex_ast::TypeExpr>, _used: &HashSet<String>) -> bool {
    // Type annotations never count as a use of a binding.
    let _ = ty;
    false
}

fn collect_block_idents(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_stmt_idents(stmt, out);
    }
}

/// Collects identifiers in a block while skipping the declared names of
/// bindings, so a binding does not count as a use of itself.
fn collect_block_idents_excluding(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        if let StmtKind::Let { init, .. } = &stmt.kind {
            collect_expr_idents(init, out);
            continue;
        }
        collect_stmt_idents(stmt, out);
    }
}

fn collect_stmt_idents(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Let { init, .. } => collect_expr_idents(init, out),
        StmtKind::Assign { name, value } => {
            out.insert(name.clone());
            collect_expr_idents(value, out);
        }
        StmtKind::AssignField {
            base, path, value, ..
        } => {
            out.insert(base.clone());
            for field in path {
                out.insert(field.clone());
            }
            collect_expr_idents(value, out);
        }
        StmtKind::AssignIndex { base, index, value } => {
            out.insert(base.clone());
            collect_expr_idents(index, out);
            collect_expr_idents(value, out);
        }
        StmtKind::If {
            branches,
            else_branch,
        } => {
            for (cond, body) in branches {
                collect_expr_idents(cond, out);
                collect_block_idents(body, out);
            }
            if let Some(body) = else_branch {
                collect_block_idents(body, out);
            }
        }
        StmtKind::While { cond, body } => {
            collect_expr_idents(cond, out);
            collect_block_idents(body, out);
        }
        StmtKind::For {
            var,
            start,
            end,
            body,
        } => {
            out.insert(var.clone());
            collect_expr_idents(start, out);
            collect_expr_idents(end, out);
            collect_block_idents(body, out);
        }
        StmtKind::Return(Some(expr)) => collect_expr_idents(expr, out),
        StmtKind::Match { scrutinee, arms } => {
            collect_expr_idents(scrutinee, out);
            for arm in arms {
                collect_block_idents(&arm.body, out);
            }
        }
        StmtKind::Expr(expr) => collect_expr_idents(expr, out),
        _ => {}
    }
}

fn collect_block_calls(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        collect_stmt_calls(stmt, out);
    }
}

fn collect_stmt_calls(stmt: &Stmt, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Let { init, .. } => collect_expr_calls(init, out),
        StmtKind::Assign { value, .. } => collect_expr_calls(value, out),
        StmtKind::AssignField { value, .. } => collect_expr_calls(value, out),
        StmtKind::AssignIndex { index, value, .. } => {
            collect_expr_calls(index, out);
            collect_expr_calls(value, out);
        }
        StmtKind::If {
            branches,
            else_branch,
        } => {
            for (cond, body) in branches {
                collect_expr_calls(cond, out);
                collect_block_calls(body, out);
            }
            if let Some(body) = else_branch {
                collect_block_calls(body, out);
            }
        }
        StmtKind::While { cond, body } => {
            collect_expr_calls(cond, out);
            collect_block_calls(body, out);
        }
        StmtKind::For {
            start, end, body, ..
        } => {
            collect_expr_calls(start, out);
            collect_expr_calls(end, out);
            collect_block_calls(body, out);
        }
        StmtKind::Return(Some(expr)) => collect_expr_calls(expr, out),
        StmtKind::Match { scrutinee, arms } => {
            collect_expr_calls(scrutinee, out);
            for arm in arms {
                collect_block_calls(&arm.body, out);
            }
        }
        StmtKind::Expr(expr) => collect_expr_calls(expr, out),
        _ => {}
    }
}

fn collect_expr_idents(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            out.insert(name.clone());
        }
        ExprKind::Unary { operand, .. } => collect_expr_idents(operand, out),
        ExprKind::Binary { left, right, .. } => {
            collect_expr_idents(left, out);
            collect_expr_idents(right, out);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                collect_expr_idents(arg, out);
            }
        }
        ExprKind::Cast { expr, .. } => collect_expr_idents(expr, out),
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                collect_expr_idents(item, out);
            }
        }
        ExprKind::Index { base, index } => {
            collect_expr_idents(base, out);
            collect_expr_idents(index, out);
        }
        ExprKind::Field { base, name, .. } => {
            collect_expr_idents(base, out);
            out.insert(name.clone());
        }
        _ => {}
    }
}

fn collect_expr_calls(expr: &Expr, out: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Call { callee, args, .. } => {
            out.insert(callee.clone());
            for arg in args {
                collect_expr_calls(arg, out);
            }
        }
        ExprKind::Unary { operand, .. } => collect_expr_calls(operand, out),
        ExprKind::Binary { left, right, .. } => {
            collect_expr_calls(left, out);
            collect_expr_calls(right, out);
        }
        ExprKind::Cast { expr, .. } => collect_expr_calls(expr, out),
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                collect_expr_calls(item, out);
            }
        }
        ExprKind::Index { base, index } => {
            collect_expr_calls(base, out);
            collect_expr_calls(index, out);
        }
        ExprKind::Field { base, .. } => collect_expr_calls(base, out),
        _ => {}
    }
}

/// Unused-import detection needs the identifier set of the whole program, so it
/// is exposed for the CLI which owns the `Program` value.
pub fn imported_names(program: &Program) -> HashMap<String, Span> {
    let mut names = HashMap::new();
    for item in &program.uses {
        if let Some(last) = item.path.last() {
            names.insert(last.clone(), item.span);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warnings(source: &str) -> Vec<String> {
        let result = lint_source(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result
            .warnings
            .into_iter()
            .map(|warning| format!("{} {}", warning.code, warning.message))
            .collect()
    }

    #[test]
    fn flags_unused_binding() {
        let found = warnings("fn main():\n    let unused = 1\n    print(2)\n");
        assert!(
            found.iter().any(|item| item.starts_with("NXL001")),
            "{found:?}"
        );
    }

    #[test]
    fn ignores_underscore_binding() {
        let found = warnings("fn main():\n    let _ignored = 1\n    print(2)\n");
        assert!(
            !found.iter().any(|item| item.starts_with("NXL001")),
            "{found:?}"
        );
    }

    #[test]
    fn accepts_used_binding() {
        let found = warnings("fn main():\n    let value = 1\n    print(value)\n");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn flags_unused_private_function() {
        let found = warnings("fn helper():\n    print(1)\nfn main():\n    print(2)\n");
        assert!(
            found.iter().any(|item| item.starts_with("NXL002")),
            "{found:?}"
        );
    }

    #[test]
    fn accepts_called_function() {
        let found = warnings("fn helper():\n    print(1)\nfn main():\n    helper()\n");
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn flags_self_comparison() {
        let found = warnings("fn main():\n    let x = 1\n    print(x == x)\n");
        assert!(
            found.iter().any(|item| item.starts_with("NXL003")),
            "{found:?}"
        );
    }

    #[test]
    fn flags_constant_condition() {
        let found = warnings("fn main():\n    if true:\n        print(1)\n");
        assert!(
            found.iter().any(|item| item.starts_with("NXL004")),
            "{found:?}"
        );
    }

    #[test]
    fn skips_files_that_do_not_parse() {
        let result = lint_source("fn main(:\n");
        assert!(!result.diagnostics.is_empty());
        assert!(result.warnings.is_empty());
    }
}
