//! NEX source formatter.
//!
//! The formatter works on the AST rather than on raw text: it parses the file
//! and prints it again with a canonical layout. `#` comments are re-attached
//! from the lexer's trivia table (before the statement they annotate, or
//! appended to the statement on the same source line), so formatting a
//! commented file keeps its comments.

use nex_ast::{
    BinOp, Block, ConstDecl, EnumDecl, Expr, ExprKind, FnDecl, GlobalDecl, ImplBlock, MatchArm,
    Param, Program, Stmt, StmtKind, StructDecl, TraitDecl, TypeAliasDecl, TypeExpr, UnaryOp,
    UseDecl,
};
use nex_diagnostics::{Diagnostic, Span};
use nex_lexer::Comment;

const INDENT: &str = "    ";

/// Result of formatting a single source file.
pub struct FormatResult {
    /// Formatted source. Equals the input when the file could not be formatted.
    pub source: String,
    /// Lexer or parser diagnostics. Empty when formatting succeeded.
    pub diagnostics: Vec<Diagnostic>,
    /// True when the input contained a `#` comment outside of a string literal.
    /// Comments are preserved in the output; this flag only reports their
    /// presence, for callers that want to warn.
    pub had_comments: bool,
    /// Set when the file parsed but the formatter cannot reproduce it yet.
    /// The source is returned unchanged in that case.
    pub unsupported: Option<String>,
}

impl FormatResult {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// Formats `source`, returning the canonical layout.
pub fn format_source(source: &str) -> FormatResult {
    let had_comments = contains_comment(source);
    let lexed = nex_lexer::Lexer::new(source).lex();
    if !lexed.diagnostics.is_empty() {
        return FormatResult {
            source: source.to_string(),
            diagnostics: lexed.diagnostics,
            had_comments,
            unsupported: None,
        };
    }

    let parsed = nex_parser::parse(&lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        return FormatResult {
            source: source.to_string(),
            diagnostics: parsed.diagnostics,
            had_comments,
            unsupported: None,
        };
    }

    if !parsed.program.modules.is_empty() {
        return FormatResult {
            source: source.to_string(),
            diagnostics: Vec::new(),
            had_comments,
            unsupported: Some(
                "module blocks are not preserved by the formatter yet; the file was left untouched"
                    .to_string(),
            ),
        };
    }

    let mut printer = Printer::new(source);
    printer.program(&parsed.program);

    FormatResult {
        source: printer.finish(lexed.comments),
        diagnostics: Vec::new(),
        had_comments,
        unsupported: None,
    }
}

/// True when the source contains a `#` that is not inside a string or char.
pub fn contains_comment(source: &str) -> bool {
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '#' => return true,
            '"' | '\'' => {
                let quote = ch;
                while let Some(inner) = chars.next() {
                    if inner == '\\' {
                        chars.next();
                    } else if inner == quote {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

struct Printer<'a> {
    lines: Vec<String>,
    /// Span of the node that produced each line, used to place comments.
    nodes: Vec<Option<Span>>,
    indent: usize,
    source: &'a str,
}

impl<'a> Printer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            lines: Vec::new(),
            nodes: Vec::new(),
            indent: 0,
            source,
        }
    }

    /// True when the whitespace run before `offset` contains a blank line.
    fn has_blank_line_before(&self, offset: usize) -> bool {
        let bytes = self.source.as_bytes();
        let mut index = offset.min(bytes.len());
        let mut newlines = 0;
        while index > 0 {
            match bytes[index - 1] {
                b'\n' => newlines += 1,
                b' ' | b'\t' | b'\r' => {}
                _ => break,
            }
            index -= 1;
        }
        newlines >= 2
    }

    fn finish(mut self, comments: Vec<Comment>) -> String {
        let mut kept: Vec<(String, Option<Span>)> =
            self.lines.drain(..).zip(self.nodes.drain(..)).collect();
        let starts = line_starts(self.source);

        let mut cursor = 0usize;
        for comment in &comments {
            let comment_line = line_index(&starts, comment.span.start);
            let mut target: Option<CommentTarget> = None;
            for (index, (_, node)) in kept.iter().enumerate().skip(cursor) {
                let Some(span) = node else { continue };
                let node_line = line_index(&starts, span.start);
                if node_line == comment_line && comment.span.start >= span.start {
                    target = Some(CommentTarget::Trailing(index));
                    break;
                }
                if span.start > comment.span.start {
                    target = Some(CommentTarget::Before(index));
                    break;
                }
            }
            match target {
                Some(CommentTarget::Trailing(index)) => {
                    kept[index].0.push_str(&format!("  {}", comment.text));
                }
                Some(CommentTarget::Before(index)) => {
                    let indent: String = kept[index]
                        .0
                        .chars()
                        .take_while(|character| *character == ' ')
                        .collect();
                    kept.insert(index, (format!("{indent}{}", comment.text), None));
                    cursor = index + 1;
                }
                None => kept.push((comment.text.clone(), None)),
            }
        }

        let mut out = kept
            .into_iter()
            .map(|(line, _)| line)
            .collect::<Vec<_>>()
            .join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    fn push_node<S: Into<String>>(&mut self, text: S, node: Option<Span>) {
        let padding = INDENT.repeat(self.indent);
        self.lines.push(format!("{padding}{}", text.into()));
        self.nodes.push(node);
    }

    fn blank(&mut self) {
        if self
            .lines
            .last()
            .is_some_and(|line| !line.trim().is_empty())
        {
            self.lines.push(String::new());
            self.nodes.push(None);
        }
    }

    fn program(&mut self, program: &Program) {
        let mut first = true;
        for module in &program.modules {
            if !first {
                self.blank();
            }
            self.push_node(format!("module {module}:"), None);
            first = false;
        }
        for item in &program.uses {
            if !first {
                self.blank();
            }
            self.use_decl(item);
            first = false;
        }
        for item in &program.globals {
            if !first {
                self.blank();
            }
            self.global_decl(item);
            first = false;
        }
        for item in &program.structs {
            if !first {
                self.blank();
            }
            self.struct_decl(item);
            first = false;
        }
        for item in &program.enums {
            if !first {
                self.blank();
            }
            self.enum_decl(item);
            first = false;
        }
        for item in &program.traits {
            if !first {
                self.blank();
            }
            self.trait_decl(item);
            first = false;
        }
        for item in &program.type_aliases {
            if !first {
                self.blank();
            }
            self.type_alias_decl(item);
            first = false;
        }
        for item in &program.consts {
            if !first {
                self.blank();
            }
            self.const_decl(item);
            first = false;
        }
        for item in &program.impls {
            if !first {
                self.blank();
            }
            self.impl_block(item);
            first = false;
        }
        for item in &program.functions {
            if !first {
                self.blank();
            }
            self.fn_decl(item);
            first = false;
        }
    }

    fn global_decl(&mut self, item: &GlobalDecl) {
        let annotation = item
            .ty
            .as_ref()
            .map(|ty| format!(": {}", type_str(ty)))
            .unwrap_or_default();
        self.push_node(
            format!("var {}{annotation} = {}", item.name, expr_str(&item.init)),
            Some(item.span),
        );
    }

    fn use_decl(&mut self, item: &UseDecl) {
        let mut text = String::from("use ");
        text.push_str(&item.path.join("."));
        if let Some(file) = &item.file {
            text.push_str(" from ");
            text.push_str(file);
        }
        self.push_node(&text, Some(item.span));
    }

    fn struct_decl(&mut self, item: &StructDecl) {
        let visibility = if item.public { "pub " } else { "" };
        self.push_node(
            format!("{visibility}struct {}:", item.name),
            Some(item.span),
        );
        self.indent += 1;
        for field in &item.fields {
            self.push_node(
                format!("{}: {}", field.name, type_str(&field.ty)),
                Some(field.span),
            );
        }
        self.indent -= 1;
    }

    fn enum_decl(&mut self, item: &EnumDecl) {
        let visibility = if item.public { "pub " } else { "" };
        self.push_node(format!("{visibility}enum {}:", item.name), Some(item.span));
        self.indent += 1;
        for variant in &item.variants {
            self.push_node(&variant.name, Some(variant.span));
        }
        self.indent -= 1;
    }

    fn trait_decl(&mut self, item: &TraitDecl) {
        let visibility = if item.public { "pub " } else { "" };
        self.push_node(format!("{visibility}trait {}:", item.name), Some(item.span));
        self.indent += 1;
        if item.methods.is_empty() {
            self.push_node("pass", Some(item.span));
        }
        for method in &item.methods {
            let params: Vec<String> = method.params.iter().map(param_str).collect();
            let mut text = format!("fn {}(self", method.name);
            if !params.is_empty() {
                text.push_str(", ");
                text.push_str(&params.join(", "));
            }
            text.push(')');
            if let Some(ret) = &method.ret {
                text.push_str(&format!(" -> {}", type_str(ret)));
            }
            self.push_node(&text, Some(method.span));
        }
        self.indent -= 1;
    }

    fn impl_block(&mut self, item: &ImplBlock) {
        let head = if item.trait_name.is_empty() {
            format!("impl {}:", item.type_name)
        } else {
            format!("impl {} for {}:", item.trait_name, item.type_name)
        };
        self.push_node(head, Some(item.span));
        self.indent += 1;
        if item.methods.is_empty() {
            self.push_node("pass", Some(item.span));
        }
        for (index, method) in item.methods.iter().enumerate() {
            if index > 0 {
                self.blank();
            }
            self.method_decl(method);
        }
        self.indent -= 1;
    }

    fn method_decl(&mut self, item: &FnDecl) {
        let mut text = format!("fn {}(", item.name);
        let params: Vec<String> = item.params.iter().skip(1).map(param_str).collect();
        text.push_str("self");
        if !params.is_empty() {
            text.push_str(", ");
            text.push_str(&params.join(", "));
        }
        text.push(')');
        if let Some(ret) = &item.ret {
            text.push_str(&format!(" -> {}", type_str(ret)));
        }
        text.push(':');
        self.push_node(&text, Some(item.span));
        self.indent += 1;
        self.block(&item.body);
        self.indent -= 1;
    }

    fn type_alias_decl(&mut self, item: &TypeAliasDecl) {
        let visibility = if item.public { "pub " } else { "" };
        self.push_node(
            format!("{visibility}type {} = {}", item.name, type_str(&item.ty)),
            Some(item.span),
        );
    }

    fn const_decl(&mut self, item: &ConstDecl) {
        let visibility = if item.public { "pub " } else { "" };
        let annotation = item
            .ty
            .as_ref()
            .map(|ty| format!(": {}", type_str(ty)))
            .unwrap_or_default();
        self.push_node(
            format!(
                "{visibility}const {}{annotation} = {}",
                item.name,
                expr_str(&item.init)
            ),
            Some(item.span),
        );
    }

    fn fn_decl(&mut self, item: &FnDecl) {
        let mut text = String::new();
        if item.public {
            text.push_str("pub ");
        }
        if let Some(abi) = &item.extern_abi {
            text.push_str(&format!("extern \"{abi}\" "));
        }
        text.push_str(&format!("fn {}(", item.name));
        let params: Vec<String> = item.params.iter().map(param_str).collect();
        text.push_str(&params.join(", "));
        text.push(')');
        if let Some(ret) = &item.ret {
            text.push_str(&format!(" -> {}", type_str(ret)));
        }
        text.push(':');
        self.push_node(&text, Some(item.span));
        self.indent += 1;
        self.block(&item.body);
        self.indent -= 1;
    }

    fn block(&mut self, block: &Block) {
        if block.stmts.is_empty() {
            self.push_node("pass", Some(block.span));
            return;
        }
        for (index, stmt) in block.stmts.iter().enumerate() {
            if index > 0 && self.has_blank_line_before(stmt.span.start) {
                self.blank();
            }
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let {
                name,
                names,
                mutable,
                ty,
                init,
            } => {
                let keyword = if *mutable { "var" } else { "let" };
                let annotation = ty
                    .as_ref()
                    .map(|ty| format!(": {}", type_str(ty)))
                    .unwrap_or_default();
                let target = if let Some(names) = names {
                    format!("({})", names.join(", "))
                } else {
                    name.clone()
                };
                self.push_node(
                    format!("{keyword} {target}{annotation} = {}", expr_str(init)),
                    Some(stmt.span),
                );
            }
            StmtKind::AssignMany { names, values } => {
                let targets = names.join(", ");
                let rendered: Vec<String> = values.iter().map(expr_str).collect();
                self.push_node(
                    format!("{targets} = {}", rendered.join(", ")),
                    Some(stmt.span),
                );
            }
            StmtKind::Assign { name, value } => {
                // The parser desugars `x += 1` into `x = x + 1`. Restore the
                // compound form so formatting never rewrites one into the other.
                if let Some(symbol) = compound_symbol_for(name, value) {
                    self.push_node(
                        format!("{name} {symbol}= {}", expr_str(rhs_of(value))),
                        Some(stmt.span),
                    );
                    return;
                }
                self.push_node(format!("{name} = {}", expr_str(value)), Some(stmt.span));
            }
            StmtKind::AssignField { base, path, value } => {
                let mut target = base.clone();
                for field in path {
                    target.push('.');
                    target.push_str(field);
                }
                if let Some(symbol) = compound_symbol_for(&target, value) {
                    self.push_node(
                        format!("{target} {symbol}= {}", expr_str(rhs_of(value))),
                        Some(stmt.span),
                    );
                    return;
                }
                self.push_node(format!("{target} = {}", expr_str(value)), Some(stmt.span));
            }
            StmtKind::AssignIndex { base, index, value } => {
                let target = format!("{base}[{}]", expr_str(index));
                if let Some(symbol) = compound_symbol_for(&target, value) {
                    self.push_node(
                        format!("{target} {symbol}= {}", expr_str(rhs_of(value))),
                        Some(stmt.span),
                    );
                    return;
                }
                self.push_node(format!("{target} = {}", expr_str(value)), Some(stmt.span));
            }
            StmtKind::AssignDeref { place, value } => {
                let target = format!("*{}", expr_str(place));
                self.push_node(format!("{target} = {}", expr_str(value)), Some(stmt.span));
            }
            StmtKind::If {
                branches,
                else_branch,
            } => {
                for (position, (cond, body)) in branches.iter().enumerate() {
                    let keyword = if position == 0 { "if" } else { "elif" };
                    self.push_node(format!("{} {}:", keyword, expr_str(cond)), Some(stmt.span));
                    self.indent += 1;
                    self.block(body);
                    self.indent -= 1;
                }
                if let Some(else_branch) = else_branch {
                    self.push_node("else:", Some(stmt.span));
                    self.indent += 1;
                    self.block(else_branch);
                    self.indent -= 1;
                }
            }
            StmtKind::While { cond, body } => {
                self.push_node(format!("while {}:", expr_str(cond)), Some(stmt.span));
                self.indent += 1;
                self.block(body);
                self.indent -= 1;
            }
            StmtKind::For {
                var,
                start,
                end,
                body,
            } => {
                self.push_node(
                    format!("for {var} in {}..{}:", expr_str(start), expr_str(end)),
                    Some(stmt.span),
                );
                self.indent += 1;
                self.block(body);
                self.indent -= 1;
            }
            StmtKind::Return(value) => match value {
                Some(value) => {
                    self.push_node(format!("return {}", expr_str(value)), Some(stmt.span))
                }
                None => self.push_node("return", Some(stmt.span)),
            },
            StmtKind::Match { scrutinee, arms } => {
                self.push_node(format!("match {}:", expr_str(scrutinee)), Some(stmt.span));
                self.indent += 1;
                for arm in arms {
                    self.match_arm(arm);
                }
                self.indent -= 1;
            }
            StmtKind::Unsafe(block) => {
                self.push_node("unsafe:".to_string(), Some(stmt.span));
                self.indent += 1;
                self.block(block);
                self.indent -= 1;
            }
            StmtKind::Defer(block) => {
                self.push_node("defer:".to_string(), Some(stmt.span));
                self.indent += 1;
                self.block(block);
                self.indent -= 1;
            }
            StmtKind::Pass => self.push_node("pass", Some(stmt.span)),
            StmtKind::Break => self.push_node("break", Some(stmt.span)),
            StmtKind::Continue => self.push_node("continue", Some(stmt.span)),
            StmtKind::Expr(expr) => self.push_node(expr_str(expr), Some(stmt.span)),
        }
    }

    fn match_arm(&mut self, arm: &MatchArm) {
        let head = match &arm.pattern {
            nex_ast::Pattern::Else => "else".to_string(),
            nex_ast::Pattern::Variant(name) => format!("case {name}"),
            nex_ast::Pattern::Int(value) => format!("case {value}"),
            nex_ast::Pattern::Range(start, end) => format!("case {start}..{end}"),
            nex_ast::Pattern::Bool(value) => format!("case {value}"),
            nex_ast::Pattern::Char(value) => format!("case {value:?}"),
            nex_ast::Pattern::Str(value) => format!("case {value:?}"),
        };
        let text = if let Some(guard) = &arm.guard {
            format!("{head} if {}:", expr_str(guard))
        } else {
            format!("{head}:")
        };
        self.push_node(text, Some(arm.span));
        self.indent += 1;
        self.block(&arm.body);
        self.indent -= 1;
    }
}

/// Returns the compound operator when `value` is `target = target op rhs`.
fn compound_symbol_for(target: &str, value: &Expr) -> Option<&'static str> {
    let ExprKind::Binary { op, left, .. } = &value.kind else {
        return None;
    };
    if expr_str(left) != target {
        return None;
    }
    compound_op(op)
}

fn compound_op(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add => Some("+"),
        BinOp::Sub => Some("-"),
        BinOp::Mul => Some("*"),
        BinOp::Div => Some("/"),
        BinOp::Rem => Some("%"),
        _ => None,
    }
}

fn rhs_of(value: &Expr) -> &Expr {
    match &value.kind {
        ExprKind::Binary { right, .. } => right,
        _ => value,
    }
}
/// Floats always print with a fractional part so `2.0` never becomes `2`.
fn format_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

fn param_str(param: &Param) -> String {
    format!("{}: {}", param.name, type_str(&param.ty))
}

fn type_str(ty: &TypeExpr) -> String {
    if let Some((element, len)) = &ty.array {
        return match len {
            Some(len) => format!("[{}; {len}]", type_str(element)),
            None => format!("[{}]", type_str(element)),
        };
    }
    if let Some(tuple) = &ty.tuple {
        let parts: Vec<String> = tuple.iter().map(type_str).collect();
        return format!("({})", parts.join(", "));
    }
    if ty.name == "&" || ty.name == "&mut" {
        if let Some(inner) = ty.generic_args.first() {
            if ty.name == "&mut" {
                return format!("&mut {}", type_str(inner));
            }
            return format!("&{}", type_str(inner));
        }
    }
    ty.name.clone()
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_index(starts: &[usize], offset: usize) -> usize {
    starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1)
}

enum CommentTarget {
    Trailing(usize),
    Before(usize),
}

fn expr_str(expr: &Expr) -> String {
    let mut out = String::new();
    write_expr(expr, 0, &mut out);
    out
}

const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_BIT_OR: u8 = 3;
const PREC_BIT_XOR: u8 = 4;
const PREC_BIT_AND: u8 = 5;
const PREC_SHIFT: u8 = 6;
const PREC_COMPARE: u8 = 7;
const PREC_ADD: u8 = 8;
const PREC_MUL: u8 = 9;
const PREC_UNARY: u8 = 10;
const PREC_PRIMARY: u8 = 11;

fn precedence(kind: &ExprKind) -> u8 {
    match kind {
        ExprKind::Binary { op, .. } => match op {
            BinOp::Or => PREC_OR,
            BinOp::And => PREC_AND,
            BinOp::BitOr => PREC_BIT_OR,
            BinOp::BitXor => PREC_BIT_XOR,
            BinOp::BitAnd => PREC_BIT_AND,
            BinOp::Shl | BinOp::Shr => PREC_SHIFT,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => PREC_COMPARE,
            BinOp::Add | BinOp::Sub => PREC_ADD,
            BinOp::Mul | BinOp::Div | BinOp::Rem => PREC_MUL,
        },
        ExprKind::Unary { .. } => PREC_UNARY,
        ExprKind::Cast { .. } => PREC_UNARY,
        _ => PREC_PRIMARY,
    }
}

fn write_expr(expr: &Expr, parent_precedence: u8, out: &mut String) {
    let own = precedence(&expr.kind);
    let needs_parens = own < parent_precedence;
    if needs_parens {
        out.push('(');
    }
    write_expr_inner(expr, own, out);
    if needs_parens {
        out.push(')');
    }
}

fn write_expr_inner(expr: &Expr, own: u8, out: &mut String) {
    match &expr.kind {
        ExprKind::Int(value) => out.push_str(&value.to_string()),
        ExprKind::Float(value) => out.push_str(&format_float(*value)),
        ExprKind::Str(value) => write_string(value, out),
        ExprKind::Char(value) => {
            out.push('\'');
            match value {
                '\'' => out.push_str("\\'"),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                other => out.push(*other),
            }
            out.push('\'');
        }
        ExprKind::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        ExprKind::Ident(name) => out.push_str(name),
        ExprKind::Unary { op, operand } => {
            let symbol = match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "not ",
                UnaryOp::BitNot => "~",
            };
            out.push_str(symbol);
            write_expr(operand, own, out);
        }
        ExprKind::Binary { op, left, right } => {
            write_expr(left, own, out);
            out.push(' ');
            out.push_str(op.symbol());
            out.push(' ');
            write_expr(right, own + 1, out);
        }
        ExprKind::Call { callee, args, .. } => {
            out.push_str(callee);
            out.push('(');
            let rendered: Vec<String> = args.iter().map(expr_str).collect();
            out.push_str(&rendered.join(", "));
            out.push(')');
        }
        ExprKind::Cast { expr, ty } => {
            write_expr(expr, own, out);
            out.push_str(" as ");
            out.push_str(&type_str(ty));
        }
        ExprKind::Tuple(items) => {
            out.push('(');
            let rendered: Vec<String> = items.iter().map(expr_str).collect();
            out.push_str(&rendered.join(", "));
            out.push(')');
        }
        ExprKind::Array(items) => {
            out.push('[');
            let rendered: Vec<String> = items.iter().map(expr_str).collect();
            out.push_str(&rendered.join(", "));
            out.push(']');
        }
        ExprKind::Index { base, index } => {
            write_expr(base, own, out);
            out.push('[');
            out.push_str(&expr_str(index));
            out.push(']');
        }
        ExprKind::Field { base, name, .. } => {
            write_expr(base, own, out);
            out.push('.');
            out.push_str(name);
        }
        ExprKind::Try(inner) => {
            write_expr(inner, own, out);
            out.push('?');
        }
        ExprKind::Borrow { mutable, place } => {
            out.push('&');
            if *mutable {
                out.push_str("mut ");
            }
            write_expr(place, own, out);
        }
        ExprKind::Deref(inner) => {
            out.push('*');
            write_expr(inner, own, out);
        }
        ExprKind::Error => out.push_str("/* invalid expression */"),
    }
}

fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Formats `source`, returning the new text when it changed.
pub fn format_if_changed(source: &str) -> Option<String> {
    let result = format_source(source);
    if result.has_errors() {
        return None;
    }
    (result.source != source).then_some(result.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(source: &str) -> String {
        let result = format_source(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        result.source
    }

    #[test]
    fn formats_functions_and_bindings() {
        let source = "fn add(a: i64,b: i64)->i64:\n    return a+b\nfn main():\n    var x=1\n    print(add(1,2))\n";
        let expected = "fn add(a: i64, b: i64) -> i64:\n    return a + b\n\nfn main():\n    var x = 1\n    print(add(1, 2))\n";
        assert_eq!(format(source), expected);
    }

    #[test]
    fn formatting_is_idempotent() {
        let source =
            "fn main():\n    var total=0\n    for i in 1..5:\n        total+=i\n    print(total)\n";
        let once = format(source);
        let twice = format(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn collapses_extra_blank_lines() {
        let source = "fn a():\n    print(1)\n\n\n\nfn b():\n    print(2)\n";
        let expected = "fn a():\n    print(1)\n\nfn b():\n    print(2)\n";
        assert_eq!(format(source), expected);
    }

    #[test]
    fn parenthesises_by_precedence() {
        let source = "fn main():\n    var x = 1 + 2 * 3\n    var y = (1 + 2) * 3\n    print(x)\n    print(y)\n";
        let expected = "fn main():\n    var x = 1 + 2 * 3\n    var y = (1 + 2) * 3\n    print(x)\n    print(y)\n";
        assert_eq!(format(source), expected);
    }

    #[test]
    fn formats_structs_enums_and_match() {
        let source = "struct Point:\n    x: i64\n    y: i64\nenum Color:\n    Red\n    Green\nfn main():\n    var p = Point(1, 2)\n    print(p.x)\n";
        let out = format(source);
        assert!(
            out.contains("struct Point:\n    x: i64\n    y: i64"),
            "{out}"
        );
        assert!(out.contains("enum Color:\n    Red\n    Green"), "{out}");
    }
    #[test]
    fn preserves_compound_field_assignment() {
        let source = "struct Item:\n    price: f64\nfn main():\n    var item = Item(1.0)\n    item.price -= 0.5\n    print(item.price)\n";
        let out = format(source);
        assert!(out.contains("item.price -= 0.5"), "{out}");
    }

    #[test]
    fn preserves_standalone_comments() {
        let source = "# header\nfn main():\n    # greeting\n    print(\"hi\")\n";
        let out = format(source);
        assert!(out.contains("# header"), "{out}");
        assert!(out.contains("# greeting"), "{out}");
        assert!(out.contains("fn main():"), "{out}");
    }

    #[test]
    fn preserves_trailing_comments_on_the_same_line() {
        let source = "fn main():\n    var x = 1 # keep me\n    println(x)\n";
        let out = format(source);
        assert!(out.contains("var x = 1  # keep me"), "{out}");
    }

    #[test]
    fn preserves_globals_and_comments_around_them() {
        let source = "# module state\nvar router = 0\nvar lock = 0\n\n# helper\nfn bump():\n    router = router + 1\n";
        let out = format(source);
        assert!(out.contains("var router = 0"), "{out}");
        assert!(out.contains("var lock = 0"), "{out}");
        assert!(out.contains("# module state"), "{out}");
        assert!(
            out.find("var router").unwrap() < out.find("fn bump").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn formatting_with_comments_is_idempotent() {
        let source = "# top\nvar counter = 0 # counts\n\n# even\nfn main():\n    # first\n    counter += 1 # mutation\n    # last\n    println(counter)\n";
        let once = format(source);
        let twice = format(&once);
        assert_eq!(once, twice);
        assert_eq!(format(&once), once);
    }

    #[test]
    fn reports_comments() {
        assert!(contains_comment("fn main():\n    # note\n    print(1)\n"));
        assert!(!contains_comment(
            "fn main():\n    print(\"# not a comment\")\n"
        ));
    }

    #[test]
    fn formats_pass_statements() {
        let source = "fn noop():\n    pass\n\nfn main():\n    noop()\n";
        let expected = "fn noop():\n    pass\n\nfn main():\n    noop()\n";
        assert_eq!(format(source), expected);
    }

    #[test]
    fn formats_traits_and_impls() {
        let source = "trait Shape:\n  fn area(self)->f64\nstruct Rect:\n  w: f64\nimpl Shape for Rect:\n  fn area(self)->f64:\n    return self.w*self.w\nfn main():\n  var r=Rect(2.0)\n  print(r.area())\n";
        let expected = "struct Rect:\n    w: f64\n\ntrait Shape:\n    fn area(self) -> f64\n\nimpl Shape for Rect:\n    fn area(self) -> f64:\n        return self.w * self.w\n\nfn main():\n    var r = Rect(2.0)\n    print(r.area())\n";
        assert_eq!(format(source), expected);
    }

    #[test]
    fn rejects_invalid_source_without_panicking() {
        let result = format_source("fn main(:\n");
        assert!(result.has_errors());
        assert_eq!(result.source, "fn main(:\n");
    }
}
