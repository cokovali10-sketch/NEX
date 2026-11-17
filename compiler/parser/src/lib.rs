//! Recursive-descent parser for NEX 0.1.
//!
//! The parser is intentionally strict but performs statement-level recovery:
//! after a syntax error it skips to the next line or block boundary, which lets
//! the compiler report more than one independent error per file.

use nex_ast::{
    BinOp, Block, ConstDecl, EnumDecl, Expr, ExprKind, FieldDecl, FnDecl, MatchArm, Param, Program,
    Stmt, StmtKind, StructDecl, TypeExpr, UnaryOp, UseDecl, VariantDecl,
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
        let mut modules = Vec::new();
        let mut uses = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut consts = Vec::new();
        let mut functions = Vec::new();

        while !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            let is_public = self.eat(&TokenKind::Pub);

            if self.check(&TokenKind::Struct) {
                if let Some(mut definition) = self.parse_struct() {
                    definition.public = is_public;
                    structs.push(definition);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Enum) {
                if let Some(mut definition) = self.parse_enum() {
                    definition.public = is_public;
                    enums.push(definition);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Const) {
                if let Some(mut constant) = self.parse_const() {
                    constant.public = is_public;
                    consts.push(constant);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Extern) {
                if let Some(mut function) = self.parse_extern_function() {
                    function.public = is_public;
                    functions.push(function);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Fn) {
                if let Some(mut function) = self.parse_function() {
                    function.public = is_public;
                    functions.push(function);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Module) {
                if let Some((name, funcs, consts2, structs2, enums2)) = self.parse_module() {
                    modules.push(name);
                    functions.extend(funcs);
                    consts.extend(consts2);
                    structs.extend(structs2);
                    enums.extend(enums2);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Use) {
                if let Some(decl) = self.parse_use() {
                    uses.push(decl);
                } else {
                    self.synchronize_item();
                }
            } else {
                let token = self.peek_token().clone();
                let message = match &token.kind {
                    TokenKind::Impl
                    | TokenKind::Trait
                    | TokenKind::Import
                    | TokenKind::From
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
                modules,
                uses,
                structs,
                enums,
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
            public: false,
            extern_abi: None,
            params,
            ret,
            body,
            span,
        })
    }

    fn parse_extern_function(&mut self) -> Option<FnDecl> {
        let start = self.advance().span; // extern
        let abi = if let TokenKind::Str(value) = self.peek() {
            let value = value.clone();
            self.advance();
            Some(value)
        } else {
            Some("C".to_string())
        };

        if !self.expect(&TokenKind::Fn, "after `extern`") {
            self.synchronize_item();
            return None;
        }

        let (name, _name_span) = self.expect_ident("as a function name")?;
        let params = if self.check(&TokenKind::LParen) {
            self.advance();
            let params = self.parse_params();
            if !self.expect(&TokenKind::RParen, "after parameters") {
                self.synchronize_item();
                return None;
            }
            params
        } else {
            Vec::new()
        };

        let ret = if self.eat(&TokenKind::Arrow) {
            Some(self.parse_type())
        } else {
            None
        };

        let span = start.merge(self.peek_token().span);
        self.expect_line_end();
        let body = nex_ast::Block {
            stmts: Vec::new(),
            span,
        };
        Some(FnDecl {
            name,
            public: false,
            extern_abi: abi,
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
            public: false,
            ty,
            init,
            span,
        })
    }

    fn parse_struct(&mut self) -> Option<StructDecl> {
        let start = self.advance().span;
        let (name, _name_span) = self.expect_ident("after `struct`")?;

        if !self.expect(&TokenKind::Colon, "after struct name") {
            self.synchronize_item();
            return None;
        }
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error("NXP002", "expected an indented struct body after `:`", span);
            return None;
        }

        let mut fields = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            let Some((field_name, field_span)) = self.expect_ident("in struct field") else {
                self.recover_statement();
                continue;
            };

            if !self.expect(&TokenKind::Colon, "after struct field name") {
                self.recover_statement();
                continue;
            }

            let ty = self.parse_type();
            let span = field_span.merge(ty.span);
            self.expect_line_end();
            fields.push(FieldDecl {
                name: field_name,
                ty,
                span,
            });
        }

        let end = self.peek_token().span.end;
        self.eat(&TokenKind::Dedent);
        Some(StructDecl {
            name,
            public: false,
            fields,
            span: Span::new(start.start, end),
        })
    }

    fn parse_enum(&mut self) -> Option<EnumDecl> {
        let start = self.advance().span;
        let (name, _name_span) = self.expect_ident("after `enum`")?;

        if !self.expect(&TokenKind::Colon, "after enum name") {
            self.synchronize_item();
            return None;
        }
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error("NXP002", "expected an indented enum body after `:`", span);
            return None;
        }

        let mut variants = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }
            let Some((variant, variant_span)) = self.expect_ident("in enum variant list") else {
                self.recover_statement();
                continue;
            };
            self.expect_line_end();
            variants.push(VariantDecl {
                name: variant,
                span: variant_span,
            });
        }

        let end = self.peek_token().span.end;
        self.eat(&TokenKind::Dedent);
        Some(EnumDecl {
            name,
            public: false,
            variants,
            span: Span::new(start.start, end),
        })
    }

    fn parse_match(&mut self) -> Stmt {
        let start = self.advance().span;
        let scrutinee = self.parse_expr();
        let head_span = start.merge(scrutinee.span);

        if !self.expect(&TokenKind::Colon, "after `match` expression") {
            self.recover_statement();
            return Stmt {
                kind: StmtKind::Match {
                    scrutinee,
                    arms: Vec::new(),
                },
                span: head_span,
            };
        }
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error("NXP002", "expected an indented match body after `:`", span);
            return Stmt {
                kind: StmtKind::Match {
                    scrutinee,
                    arms: Vec::new(),
                },
                span: head_span,
            };
        }

        let mut arms = Vec::new();
        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            let arm_start = self.peek_token().span;
            let pattern = if self.eat(&TokenKind::Case) {
                let Some((variant, _variant_span)) = self.expect_ident("after `case`") else {
                    self.recover_statement();
                    continue;
                };
                Some(variant)
            } else if self.eat(&TokenKind::Else) {
                None
            } else {
                let span = self.peek_token().span;
                self.error("NXP013", "expected `case` or `else` in a match body", span);
                self.recover_statement();
                continue;
            };

            let body = self.parse_arm_body();
            let span = arm_start.merge(body.span);
            arms.push(MatchArm {
                pattern,
                body,
                span,
            });
        }

        let end = self.peek_token().span.end;
        self.eat(&TokenKind::Dedent);
        Stmt {
            kind: StmtKind::Match { scrutinee, arms },
            span: head_span.merge(Span::new(end, end)),
        }
    }

    fn parse_arm_body(&mut self) -> Block {
        let start = self.peek_token().span.start;
        self.expect(&TokenKind::Colon, "after a match arm pattern");
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error(
                "NXP002",
                "expected an indented match arm body after `:`",
                span,
            );
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

    #[allow(clippy::type_complexity)]
    fn parse_module(
        &mut self,
    ) -> Option<(
        String,
        Vec<FnDecl>,
        Vec<ConstDecl>,
        Vec<StructDecl>,
        Vec<EnumDecl>,
    )> {
        let start = self.advance().span; // module
        let (name, _name_span) = self.expect_ident("after `module`")?;

        if !self.expect(&TokenKind::Colon, "after module name") {
            self.synchronize_item();
            return None;
        }
        self.expect_line_end();

        if !self.eat(&TokenKind::Indent) {
            let span = self.peek_token().span;
            self.error("NXP002", "expected an indented module body after `:`", span);
            return None;
        }

        let mut functions = Vec::new();
        let mut consts = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();

        while !self.check(&TokenKind::Dedent) && !self.check(&TokenKind::Eof) {
            if self.eat(&TokenKind::Newline) {
                continue;
            }

            let is_public = self.eat(&TokenKind::Pub);

            if self.check(&TokenKind::Struct) {
                if let Some(mut definition) = self.parse_struct() {
                    definition.name = format!("{name}.{}", definition.name);
                    definition.public = is_public;
                    structs.push(definition);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Enum) {
                if let Some(mut definition) = self.parse_enum() {
                    definition.name = format!("{name}.{}", definition.name);
                    definition.public = is_public;
                    enums.push(definition);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Const) {
                if let Some(mut constant) = self.parse_const() {
                    constant.name = format!("{name}.{}", constant.name);
                    constant.public = is_public;
                    consts.push(constant);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Extern) {
                if let Some(mut function) = self.parse_extern_function() {
                    function.name = format!("{name}.{}", function.name);
                    function.public = is_public;
                    functions.push(function);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Fn) {
                if let Some(mut function) = self.parse_function() {
                    function.name = format!("{name}.{}", function.name);
                    function.public = is_public;
                    functions.push(function);
                } else {
                    self.synchronize_item();
                }
            } else if self.check(&TokenKind::Module) {
                self.error(
                    "NXP010",
                    "nested modules are not supported in NEX 0.2".to_string(),
                    self.peek_token().span,
                );
                self.synchronize_item();
            } else {
                let token = self.peek_token().clone();
                self.error(
                    "NXP010",
                    format!("expected a declaration, found {}", token.kind.describe()),
                    token.span,
                );
                self.synchronize_item();
            }
        }

        let _end = self.peek_token().span.end;
        self.eat(&TokenKind::Dedent);
        let _ = start;
        Some((name, functions, consts, structs, enums))
    }

    fn parse_use(&mut self) -> Option<UseDecl> {
        let start = self.advance().span; // use
        let Some((first, _)) = self.expect_ident("after `use`") else {
            self.synchronize_item();
            return None;
        };
        let mut path = vec![first];
        while self.eat(&TokenKind::Dot) {
            let Some((next, _)) = self.expect_ident("in use path") else {
                self.synchronize_item();
                return None;
            };
            path.push(next);
        }
        let span = start.merge(self.peek_token().span);
        self.expect_line_end();
        Some(UseDecl {
            path,
            file: None,
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
        if self.check(&TokenKind::LParen) {
            let start = self.advance().span;
            let first = self.parse_type();
            if self.eat(&TokenKind::Comma) {
                let mut elems = vec![first];
                loop {
                    elems.push(self.parse_type());
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                let end_span = if self.check(&TokenKind::RParen) {
                    let span = self.peek_token().span;
                    self.advance();
                    span
                } else {
                    let span = self.peek_token().span;
                    self.error("NXP011", "expected `)`", span);
                    span
                };
                return TypeExpr {
                    name: String::new(),
                    array: None,
                    tuple: Some(elems),
                    span: start.merge(end_span),
                };
            }
            let end_span = if self.check(&TokenKind::RParen) {
                let span = self.peek_token().span;
                self.advance();
                span
            } else {
                let span = self.peek_token().span;
                self.error("NXP011", "expected `)`", span);
                span
            };
            return TypeExpr {
                name: first.name,
                array: first.array,
                tuple: first.tuple,
                span: first.span.merge(end_span),
            };
        }

        if self.check(&TokenKind::LBracket) {
            let start = self.advance().span; // [
            let elem = self.parse_type();
            if !self.expect(&TokenKind::Semicolon, "in array type") {
                return TypeExpr {
                    name: "<error>".to_string(),
                    array: None,
                    tuple: None,
                    span: start,
                };
            }
            // `[T]` is the growable form, `[T; N]` the fixed-size one.
            let size = match self.peek() {
                TokenKind::Int(value) => {
                    let value = *value;
                    self.advance();
                    Some(value as usize)
                }
                _ => None,
            };
            let end_span = if self.check(&TokenKind::RBracket) {
                let span = self.peek_token().span;
                self.advance();
                span
            } else {
                let span = self.peek_token().span;
                self.error("NXP011", "expected `]`", span);
                span
            };
            return TypeExpr {
                name: String::new(),
                array: Some((Box::new(elem), size)),
                tuple: None,
                span: start.merge(end_span),
            };
        }

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
            TypeExpr {
                name,
                array: None,
                tuple: None,
                span,
            }
        } else {
            let span = self.peek_token().span;
            TypeExpr {
                name: "<error>".to_string(),
                array: None,
                tuple: None,
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
            TokenKind::Match => Some(self.parse_match()),
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
            TokenKind::Pass => {
                let start = self.advance().span;
                self.expect_line_end();
                Some(Stmt {
                    kind: StmtKind::Pass,
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
        let expr = self.parse_expr();

        let is_assignment = self.check(&TokenKind::Eq) || compound_bin_op(self.peek()).is_some();

        if is_assignment {
            let place = expr.clone();
            let span = place.span;
            let compound = if self.check(&TokenKind::Eq) {
                None
            } else {
                compound_bin_op(self.peek())
            };
            self.advance(); // = or compound op

            let rhs = self.parse_expr();
            let span = span.merge(rhs.span);
            self.expect_line_end();

            let value = match compound {
                Some(op) => Expr {
                    kind: ExprKind::Binary {
                        op,
                        left: Box::new(place.clone()),
                        right: Box::new(rhs),
                    },
                    span,
                },
                None => rhs,
            };

            if let Some(statement) = assignment_from_place(place, value, span) {
                return statement;
            }
        }

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
        loop {
            if self.check(&TokenKind::LParen) {
                let callee = match &expr.kind {
                    ExprKind::Ident(name) => name.clone(),
                    ExprKind::Field { .. } => {
                        // Support calls through module paths such as `math.add(...)`.
                        let mut parts = Vec::new();
                        let mut current = expr.clone();
                        while let ExprKind::Field {
                            base: inner,
                            name: field,
                            ..
                        } = &current.kind
                        {
                            parts.push(field.clone());
                            current = *inner.clone();
                        }
                        if let ExprKind::Ident(root) = &current.kind {
                            parts.push(root.clone());
                            parts.reverse();
                            parts.join(".")
                        } else {
                            let span = self.peek_token().span;
                            self.error(
                                "NXP004",
                                "only named functions can be called in NEX 0.1",
                                span,
                            );
                            break;
                        }
                    }
                    _ => {
                        let span = self.peek_token().span;
                        self.error(
                            "NXP004",
                            "only named functions can be called in NEX 0.1",
                            span,
                        );
                        break;
                    }
                };
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
                continue;
            }

            if self.check(&TokenKind::Dot) {
                self.advance();
                let (name, name_span) = if let TokenKind::Ident(name) = self.peek() {
                    let name = name.clone();
                    let span = self.advance().span;
                    (name, span)
                } else if let TokenKind::Int(value) = self.peek() {
                    let value = *value;
                    let span = self.advance().span;
                    (value.to_string(), span)
                } else {
                    let span = self.peek_token().span;
                    self.error("NXP004", "expected a field name after `.`", span);
                    break;
                };
                let span = expr.span.merge(name_span);
                expr = Expr {
                    kind: ExprKind::Field {
                        base: Box::new(expr),
                        name,
                        name_span,
                    },
                    span,
                };
                continue;
            }

            if self.check(&TokenKind::LBracket) {
                let start = self.advance().span;
                let index = self.parse_expr();
                let close = self.peek_token().span;
                self.expect(&TokenKind::RBracket, "after array index");
                let span = expr.span.merge(close);
                let _ = start;
                expr = Expr {
                    kind: ExprKind::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                };
                continue;
            }

            break;
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
                if self.check(&TokenKind::RParen) {
                    let close = self.advance().span;
                    self.error("NXP014", "empty tuples are not supported in NEX 0.2", close);
                    Expr {
                        kind: ExprKind::Error,
                        span: token.span.merge(close),
                    }
                } else {
                    let first = self.parse_expr();
                    if self.eat(&TokenKind::Comma) {
                        let mut elements = vec![first];
                        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                            elements.push(self.parse_expr());
                            if !self.eat(&TokenKind::Comma) {
                                break;
                            }
                        }
                        let close = self.peek_token().span;
                        self.expect(&TokenKind::RParen, "after tuple elements");
                        Expr {
                            kind: ExprKind::Tuple(elements),
                            span: token.span.merge(close),
                        }
                    } else {
                        self.expect(&TokenKind::RParen, "after expression");
                        first
                    }
                }
            }
            TokenKind::LBracket => {
                let start = self.advance().span;
                let mut elements = Vec::new();
                while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                    elements.push(self.parse_expr());
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.peek_token().span;
                self.expect(&TokenKind::RBracket, "after array elements");
                Expr {
                    kind: ExprKind::Array(elements),
                    span: start.merge(close),
                }
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

fn compound_bin_op(kind: &TokenKind) -> Option<BinOp> {
    Some(match kind {
        TokenKind::PlusEq => BinOp::Add,
        TokenKind::MinusEq => BinOp::Sub,
        TokenKind::StarEq => BinOp::Mul,
        TokenKind::SlashEq => BinOp::Div,
        TokenKind::PercentEq => BinOp::Rem,
        _ => return None,
    })
}

fn assignment_from_place(place: Expr, value: Expr, span: Span) -> Option<Stmt> {
    match place.kind {
        ExprKind::Ident(name) => Some(Stmt {
            kind: StmtKind::Assign { name, value },
            span,
        }),
        ExprKind::Field { base, name, .. } => {
            // Collect a (possibly nested) field path such as `p.inner.x`.
            let mut path = vec![name.clone()];
            let mut current = base.as_ref();
            while let ExprKind::Field {
                base: inner,
                name: field,
                ..
            } = &current.kind
            {
                path.push(field.clone());
                current = inner.as_ref();
            }
            if let ExprKind::Ident(base_name) = &current.kind {
                path.reverse();
                Some(Stmt {
                    kind: StmtKind::AssignField {
                        base: base_name.clone(),
                        path,
                        value,
                    },
                    span,
                })
            } else {
                None
            }
        }
        ExprKind::Index { base, index } => {
            if let ExprKind::Ident(base_name) = base.kind {
                Some(Stmt {
                    kind: StmtKind::AssignIndex {
                        base: base_name,
                        index: *index,
                        value,
                    },
                    span,
                })
            } else {
                None
            }
        }
        _ => None,
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
    fn parses_struct_declaration_and_field_access() {
        let source = "struct Point:\n    x: i64\n    y: i64\n\nfn main():\n    var p = Point(1, 2)\n    p.x = p.y + 1\n";
        let result = parse_source(source);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.program.structs.len(), 1);
        assert_eq!(result.program.structs[0].name, "Point");
        assert_eq!(result.program.structs[0].fields.len(), 2);
        let StmtKind::AssignField { base, path, .. } =
            &result.program.functions[0].body.stmts[1].kind
        else {
            panic!("expected field assignment");
        };
        assert_eq!(base, "p");
        assert_eq!(path, &["x".to_string()]);
    }

    #[test]
    fn parses_field_of_expression() {
        let result = parse_source("fn main():\n    var p = Point(1, 2)\n    print(p.x)\n");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let StmtKind::Expr(expr) = &result.program.functions[0].body.stmts[1].kind else {
            panic!("expected expression statement");
        };
        let ExprKind::Call { args, .. } = &expr.kind else {
            panic!("expected call");
        };
        assert!(matches!(args[0].kind, ExprKind::Field { .. }));
    }

    #[test]
    fn desugars_compound_assignment() {
        let result = parse_source("fn main():\n    var x = 1\n    x += 2 * 3\n");
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let StmtKind::Assign { name, value } = &result.program.functions[0].body.stmts[1].kind
        else {
            panic!("expected assignment");
        };
        assert_eq!(name, "x");
        let ExprKind::Binary { op, left, right } = &value.kind else {
            panic!("expected binary expression");
        };
        assert_eq!(*op, BinOp::Add);
        assert!(matches!(left.kind, ExprKind::Ident(ref ident) if ident == "x"));
        assert!(matches!(
            right.kind,
            ExprKind::Binary { op: BinOp::Mul, .. }
        ));
    }
}
