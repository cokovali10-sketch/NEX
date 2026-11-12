//! Recursive-descent parser for NEX 0.1.
//!
//! The parser is intentionally strict but performs statement-level recovery:
//! after a syntax error it skips to the next line or block boundary, which lets
//! the compiler report more than one independent error per file.

use nex_ast::{
    BinOp, Block, ConstDecl, Expr, ExprKind, FnDecl, Param, Program, Stmt, StmtKind, TypeExpr,
    UnaryOp,
};
use nex_diagnostics::{Diagnostic, Span};
use nex_lexer::{Token, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub program: Program,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(tokens: &[Token]) -> ParseResult {
    Parser::new(tokens).parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    fn parse_program(mut self) -> ParseResult {
        let start = self.peek_token().span.start;
        let mut consts = Vec::new();
        let mut functions = Vec::new();

        while !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            if self.check(&TokenKind::Const) {
                if let Some(constant) = self.parse_const() {
                    consts.push(constant);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Fn) {
                if let Some(function) = self.parse_function() {
                    functions.push(function);
                } else {
                    self.synchronize_item();
                }
            } else {
                let token = self.peek_token().clone();
                let message = match &token.kind {
                    TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Impl
                    | TokenKind::Trait
                    | TokenKind::Import
                    | TokenKind::From
                    | TokenKind::Module
                    | TokenKind::Use
                    | TokenKind::Const
                    | TokenKind::Extern => format!(
                        "`{}` declarations are not implemented in NEX 0.1",
                        token.kind.symbol()
                    ),
                    _ => format!(
                        "expected a function declaration, found {}",
                        token.kind.describe()
                    ),
                };
                self.error("NXP010", message, token.span);
                self.synchronize_item();
            }
        }

        let end = self.peek_token().span.end;
        ParseResult {
            program: Program {
                consts,
                functions,
                span: Span::new(start, end),
            },
            diagnostics: self.diagnostics,
        }
    }

    fn parse_function(&mut self) -> Option<FnDecl> {
        let start = self.advance().span;
        let (name, _name_span) = self.expect_ident("after `fn`")?;

        if !self.expect(&TokenKind::LParen, "after function name") {
            self.synchronize_item();
            return None;
        }

        let params = self.parse_params();
        self.expect(&TokenKind::RParen, "after parameters");

        let ret = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type())
        } else {
            None
        };

        let body = self.parse_block();
        let span = start.merge(body.span);
        Some(FnDecl {
            name,
            params,
            ret,
            body,
            span,
        })
    }

    fn parse_const(&mut self) -> Option<ConstDecl> {
        let start = self.advance().span;
        let (name, _name_span) = self.expect_ident("after `const`")?;

        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };

        if !self.expect(&TokenKind::Eq, "in constant declaration") {
            self.recover_statement();
            return None;
        }

        let init = self.parse_expr();
        let span = start.merge(init.span);
        self.expect_line_end();
        Some(ConstDecl {
            name,
            ty,
            init,
            span,
        })
    }

    fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let Some((name, name_span)) = self.expect_ident("in parameter list") else {
                self.skip_until(&[TokenKind::Comma, TokenKind::RParen]);
                self.eat(&TokenKind::Comma);
                continue;
            };

            let start = name_span;
            if !self.expect(&TokenKind::Colon, "after parameter name") {
                self.skip_until(&[TokenKind::Comma, TokenKind::RParen]);
                self.eat(&TokenKind::Comma);
                continue;
            }
            let ty = self.parse_type();
            let span = start.merge(ty.span);
            params.push(Param { name, ty, span });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        params
    }

    fn parse_type(&mut self) -> TypeExpr {
        if let Some((name, span)) = self.expect_ident("as a type name") {
            if self.check(&TokenKind::Lt) {
                let generic_span = self.peek_token().span;
                self.error(
                    "NXP011",
                    "generic types are not implemented in NEX 0.1",
                    generic_span,
                );
                self.skip_balanced_angles();
            }
            TypeExpr { name, span }
        } else {
            let span = self.peek_token().span;
            TypeExpr {
                name: "<error>".to_string(),
                span,
            }
        }
    }

    fn parse_block(&mut self) -> Block {
        let start = self.peek_token().span.start;
        self.expect(&TokenKind::Colon, "before an indented block");
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error("NXP002", "expected an indented block after `:`", span);
            return Block {
                stmts: Vec::new(),
                span: Span::new(start, span.end),
            };
        }

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            let before = self.pos;
            if let Some(statement) = self.parse_statement() {
                stmts.push(statement);
            } else if self.pos == before {
                self.recover_statement();
            }
            if self.pos == before {
                self.advance();
            }
        }

        let end = self.peek_token().span.end;
        self.eat(&TokenKind::Dedent);
        Block {
            stmts,
            span: Span::new(start, end),
        }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        match self.peek() {
            TokenKind::Let | TokenKind::Var => Some(self.parse_let()),
            TokenKind::If => Some(self.parse_if()),
            TokenKind::While => Some(self.parse_while()),
            TokenKind::For => Some(self.parse_for()),
            TokenKind::Return => Some(self.parse_return()),
            TokenKind::Break => {
                let start = self.advance().span;
                self.expect_line_end();
                Some(Stmt {
                    kind: StmtKind::Break,
                    span: start,
                })
            }
            TokenKind::Continue => {
                let start = self.advance().span;
                self.expect_line_end();
                Some(Stmt {
                    kind: StmtKind::Continue,
                    span: start,
                })
            }
            TokenKind::Semicolon => {
                let token = self.advance();
                self.error(
                    "NXP003",
                    "NEX does not use semicolons; end statements with a newline",
                    token.span,
                );
                self.expect_line_end();
                None
            }
            _ => Some(self.parse_expression_or_assignment()),
        }
    }

    fn parse_let(&mut self) -> Stmt {
        let keyword = self.advance();
        let mutable = keyword.kind == TokenKind::Var;
        let start = keyword.span;

        let Some((name, _name_span)) = self.expect_ident("after binding keyword") else {
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Expr(Expr {
                    kind: ExprKind::Error,
                    span: start,
                }),
                span: start,
            };
        };

        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };

        if !self.expect(&TokenKind::Eq, "in binding declaration") {
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Expr(Expr {
                    kind: ExprKind::Error,
                    span: start,
                }),
                span: start,
            };
        }

        let init = self.parse_expr();
        let span = start.merge(init.span);
        self.expect_line_end();
        Stmt {
            kind: StmtKind::Let {
                name,
                mutable,
                ty,
                init,
            },
            span,
        }
    }

    fn parse_if(&mut self) -> Stmt {
        let start = self.advance().span;
        let cond = self.parse_expr();
        let then_block = self.parse_block();

        let mut branches = vec![(cond, then_block)];
        while self.eat(&TokenKind::Elif) {
            let cond = self.parse_expr();
            let block = self.parse_block();
            branches.push((cond, block));
        }

        let else_branch = if self.eat(&TokenKind::Else) {
            Some(self.parse_block())
        } else {
            None
        };

        let end = else_branch
            .as_ref()
            .map(|block| block.span)
            .unwrap_or_else(|| branches.last().unwrap().1.span);
        Stmt {
            kind: StmtKind::If {
                branches,
                else_branch,
            },
            span: start.merge(end),
        }
    }

    fn parse_while(&mut self) -> Stmt {
        let start = self.advance().span;
        let cond = self.parse_expr();
        let body = self.parse_block();
        let span = start.merge(body.span);
        Stmt {
            kind: StmtKind::While { cond, body },
            span,
        }
    }

    fn parse_for(&mut self) -> Stmt {
        let start = self.advance().span;
        let Some((var, _var_span)) = self.expect_ident("after `for`") else {
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Expr(Expr {
                    kind: ExprKind::Error,
                    span: start,
                }),
                span: start,
            };
        };

        if !self.expect(&TokenKind::In, "after loop variable") {
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Expr(Expr {
                    kind: ExprKind::Error,
                    span: start,
                }),
                span: start,
            };
        }

        let range_start = self.parse_expr();
        if !self.eat(&TokenKind::DotDot) {
            self.error(
                "NXP012",
                "expected `..` in `for` range",
                self.peek_token().span,
            );
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Expr(Expr {
                    kind: ExprKind::Error,
                    span: start,
                }),
                span: start,
            };
        }
        let range_end = self.parse_expr();
        let body = self.parse_block();
        let span = start.merge(body.span);
        Stmt {
            kind: StmtKind::For {
                var,
                start: range_start,
                end: range_end,
                body,
            },
            span,
        }
    }

    fn parse_return(&mut self) -> Stmt {
        let start = self.advance().span;
        let value = if matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            None
        } else {
            Some(self.parse_expr())
        };
        let end = value.as_ref().map(|expr| expr.span).unwrap_or(start);
        self.expect_line_end();
        Stmt {
            kind: StmtKind::Return(value),
            span: start.merge(end),
        }
    }

    fn parse_expression_or_assignment(&mut self) -> Stmt {
        if let TokenKind::Ident(name) = self.peek()
            && self.peek_kind(1) == &TokenKind::Eq
        {
            let name = name.clone();
            let start = self.advance().span;
            self.advance();
            let value = self.parse_expr();
            let span = start.merge(value.span);
            self.expect_line_end();
            return Stmt {
                kind: StmtKind::Assign { name, value },
                span,
            };
        }

        let expr = self.parse_expr();
        let span = expr.span;
        self.expect_line_end();
        Stmt {
            kind: StmtKind::Expr(expr),
            span,
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Expr {
        let mut left = self.parse_and();
        while self.check(&TokenKind::Or) || self.check(&TokenKind::PipePipe) {
            self.advance();
            let right = self.parse_and();
            left = binary(BinOp::Or, left, right);
        }
        left
    }

    fn parse_and(&mut self) -> Expr {
        let mut left = self.parse_not();
        while self.check(&TokenKind::And) || self.check(&TokenKind::AmpAmp) {
            self.advance();
            let right = self.parse_not();
            left = binary(BinOp::And, left, right);
        }
        left
    }

    fn parse_not(&mut self) -> Expr {
        if self.check(&TokenKind::Not) || self.check(&TokenKind::Bang) {
            let start = self.advance().span;
            let operand = self.parse_not();
            let span = start.merge(operand.span);
            Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                span,
            }
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut left = self.parse_additive();
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::LtEq => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::GtEq => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive();
            left = binary(op, left, right);
        }
        left
    }

    fn parse_additive(&mut self) -> Expr {
        let mut left = self.parse_multiplicative();
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative();
            left = binary(op, left, right);
        }
        left
    }

    fn parse_cast(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        while self.check(&TokenKind::As) {
            self.advance();
            let ty = self.parse_type();
            let span = expr.span.merge(ty.span);
            expr = Expr {
                kind: ExprKind::Cast {
                    expr: Box::new(expr),
                    ty,
                },
                span,
            };
        }
        expr
    }

    fn parse_multiplicative(&mut self) -> Expr {
        let mut left = self.parse_cast();
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            self.advance();
            let right = self.parse_cast();
            left = binary(op, left, right);
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        if self.check(&TokenKind::Minus) {
            let start = self.advance().span;
            let operand = self.parse_cast();
            let span = start.merge(operand.span);
            Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                },
                span,
            }
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        while self.check(&TokenKind::LParen) {
            let ExprKind::Ident(callee) = &expr.kind else {
                let span = self.peek_token().span;
                self.error(
                    "NXP004",
                    "only named functions can be called in NEX 0.1",
                    span,
                );
                break;
            };
            let callee = callee.clone();
            let callee_span = expr.span;
            let (args, close_span) = self.parse_arguments();
            expr = Expr {
                kind: ExprKind::Call {
                    callee,
                    callee_span,
                    args,
                },
                span: callee_span.merge(close_span),
            };
        }
        expr
    }

    fn parse_arguments(&mut self) -> (Vec<Expr>, Span) {
        let open = self.advance().span;
        let mut args = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let before = self.pos;
            args.push(self.parse_expr());
            if self.pos == before {
                self.advance();
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let close = self.peek_token().span;
        self.expect(&TokenKind::RParen, "after call arguments");
        (args, open.merge(close))
    }

    fn parse_primary(&mut self) -> Expr {
        let token = self.peek_token().clone();
        match token.kind {
            TokenKind::Int(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::Int(value),
                    span: token.span,
                }
            }
            TokenKind::Float(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::Float(value),
                    span: token.span,
                }
            }
            TokenKind::Str(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::Str(value),
                    span: token.span,
                }
            }
            TokenKind::Char(value) => {
                self.advance();
                Expr {
                    kind: ExprKind::Char(value),
                    span: token.span,
                }
            }
            TokenKind::True => {
                self.advance();
                Expr {
                    kind: ExprKind::Bool(true),
                    span: token.span,
                }
            }
            TokenKind::False => {
                self.advance();
                Expr {
                    kind: ExprKind::Bool(false),
                    span: token.span,
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                Expr {
                    kind: ExprKind::Ident(name),
                    span: token.span,
                }
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(&TokenKind::RParen, "after expression");
                expr
            }
            _ => {
                self.error(
                    "NXP001",
                    format!("expected an expression, found {}", token.kind.describe()),
                    token.span,
                );
                if !matches!(
                    token.kind,
                    TokenKind::Newline
                        | TokenKind::Dedent
                        | TokenKind::Eof
                        | TokenKind::Comma
                        | TokenKind::RParen
                ) {
                    self.advance();
                }
                Expr {
                    kind: ExprKind::Error,
                    span: token.span,
                }
            }
        }
    }

    fn expect_line_end(&mut self) {
        if self.eat(&TokenKind::Newline) {
            return;
        }
        if matches!(self.peek(), TokenKind::Dedent | TokenKind::Eof) {
            return;
        }
        let span = self.peek_token().span;
        self.error(
            "NXP005",
            format!(
                "expected a newline, found {}",
                self.peek_token().kind.describe()
            ),
            span,
        );
        while !matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.advance();
        }
        self.eat(&TokenKind::Newline);
    }

    fn recover_statement(&mut self) {
        while !matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Dedent | TokenKind::Eof
        ) {
            self.advance();
        }
        self.eat(&TokenKind::Newline);
    }

    fn synchronize_item(&mut self) {
        while !matches!(self.peek(), TokenKind::Fn | TokenKind::Eof) {
            self.advance();
        }
    }

    fn skip_until(&mut self, kinds: &[TokenKind]) {
        while !self.check(&TokenKind::Eof) && !kinds.iter().any(|kind| self.check(kind)) {
            self.advance();
        }
    }

    fn skip_balanced_angles(&mut self) {
        let mut depth = 0usize;
        while !self.check(&TokenKind::Eof) {
            match self.peek() {
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.advance();
                        return;
                    }
                }
                _ => {}
            }
            self.advance();
        }
    }

    fn expect_ident(&mut self, context: &str) -> Option<(String, Span)> {
        if let TokenKind::Ident(name) = self.peek() {
            let name = name.clone();
            let span = self.advance().span;
            Some((name, span))
        } else {
            let token = self.peek_token().clone();
            self.error(
                "NXP006",
                format!(
                    "expected an identifier {context}, found {}",
                    token.kind.describe()
                ),
                token.span,
            );
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind, context: &str) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            let token = self.peek_token().clone();
            self.error(
                "NXP007",
                format!(
                    "expected {}, found {} {context}",
                    kind.describe(),
                    token.kind.describe()
                ),
                token.span,
            );
            false
        }
    }

    fn error(&mut self, code: &str, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn advance(&mut self) -> Token {
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| Token::new(TokenKind::Eof, Span::empty(0)));
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn peek(&self) -> &TokenKind {
        &self.peek_token().kind
    }

    fn peek_kind(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map(|token| &token.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn peek_token(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("token stream always ends with EOF")
        })
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }
}

fn binary(op: BinOp, left: Expr, right: Expr) -> Expr {
    let span = left.span.merge(right.span);
    Expr {
        kind: ExprKind::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nex_lexer::Lexer;

    fn parse_source(source: &str) -> ParseResult {
        let lexed = Lexer::new(source).lex();
        assert!(
            lexed.diagnostics.is_empty(),
            "lexer diagnostics: {:?}",
            lexed.diagnostics
        );
        parse(&lexed.tokens)
    }

    #[test]
    fn parses_hello_world() {
        let result = parse_source("fn main():\n    print(\"Hello\")\n");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.program.functions.len(), 1);
        assert_eq!(result.program.functions[0].name, "main");
        assert_eq!(result.program.functions[0].body.stmts.len(), 1);
    }

    #[test]
    fn respects_operator_precedence() {
        let result = parse_source("fn main():\n    let x = 1 + 2 * 3\n");
        assert!(result.diagnostics.is_empty());
        let StmtKind::Let { init, .. } = &result.program.functions[0].body.stmts[0].kind else {
            panic!("expected let");
        };
        let ExprKind::Binary { op, right, .. } = &init.kind else {
            panic!("expected binary");
        };
        assert_eq!(*op, BinOp::Add);
        assert!(matches!(
            right.kind,
            ExprKind::Binary { op: BinOp::Mul, .. }
        ));
    }

    #[test]
    fn parses_if_elif_else_and_while() {
        let source = "fn main():\n    if true:\n        print(\"a\")\n    elif false:\n        print(\"b\")\n    else:\n        print(\"c\")\n    while false:\n        break\n";
        let result = parse_source(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.program.functions[0].body.stmts.len(), 2);
    }

    #[test]
    fn parses_function_with_types() {
        let result = parse_source("fn add(a: i64, b: i64) -> i64:\n    return a + b\n");
        assert!(result.diagnostics.is_empty());
        let function = &result.program.functions[0];
        assert_eq!(function.params.len(), 2);
        assert_eq!(function.ret.as_ref().unwrap().name, "i64");
    }

    #[test]
    fn reports_missing_block_indentation() {
        let result = parse_source("fn main():\nprint(\"x\")\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXP002"));
    }

    #[test]
    fn recovers_after_statement_error() {
        let result = parse_source("fn main():\n    let x: = 1\n    print(\"still parsed\")\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXP006"));
        assert_eq!(result.program.functions[0].body.stmts.len(), 2);
    }

    #[test]
    fn rejects_unimplemented_struct() {
        let result = parse_source("struct Point:\n    x: i64\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXP010"));
    }
}
