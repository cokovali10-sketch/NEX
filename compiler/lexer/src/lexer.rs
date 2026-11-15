use nex_diagnostics::{Diagnostic, Span};

use crate::token::{Token, TokenKind};

#[derive(Debug, Clone)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Lexer<'a> {
    source: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
    indents: Vec<usize>,
    at_line_start: bool,
    bracket_depth: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
            indents: vec![0],
            at_line_start: true,
            bracket_depth: 0,
        }
    }

    pub fn lex(mut self) -> LexResult {
        // Skip a UTF-8 byte order mark if the file starts with one.
        if self.source.starts_with('\u{feff}') {
            self.pos += '\u{feff}'.len_utf8();
        }

        while self.pos < self.source.len() {
            if self.at_line_start && self.bracket_depth == 0 {
                self.handle_line_start();
                if self.pos >= self.source.len() {
                    break;
                }
            }

            let Some(byte) = self.byte_at(self.pos) else {
                break;
            };
            match byte {
                b' ' | b'\t' => {
                    self.pos += 1;
                }
                b'\r' | b'\n' => self.consume_newline(),
                b'#' => self.skip_comment(),
                b'"' => self.scan_string(),
                b'\'' => self.scan_char(),
                b'0'..=b'9' => self.scan_number(),
                b'_' | b'a'..=b'z' | b'A'..=b'Z' => self.scan_ident(),
                _ => self.scan_operator(),
            }
        }

        if self.bracket_depth > 0 {
            self.diagnostics.push(Diagnostic::error(
                "NXL012",
                "unclosed bracket at end of file",
                Span::empty(self.source.len()),
            ));
        }

        if self
            .tokens
            .last()
            .is_some_and(|token| token.kind != TokenKind::Newline)
        {
            let position = self.source.len();
            self.push(TokenKind::Newline, Span::empty(position));
        }

        while self.indents.len() > 1 {
            self.indents.pop();
            let position = self.source.len();
            self.push(TokenKind::Dedent, Span::empty(position));
        }

        let position = self.source.len();
        self.push(TokenKind::Eof, Span::empty(position));

        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn byte_at(&self, position: usize) -> Option<u8> {
        self.source.as_bytes().get(position).copied()
    }

    fn current_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn push(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token::new(kind, span));
    }

    fn handle_line_start(&mut self) {
        let start = self.pos;
        let mut indent = 0usize;
        loop {
            match self.byte_at(self.pos) {
                Some(b' ') => {
                    indent += 1;
                    self.pos += 1;
                }
                Some(b'\t') => {
                    self.diagnostics.push(Diagnostic::error(
                        "NXL004",
                        "tabs are not allowed for indentation; use four spaces",
                        Span::new(self.pos, self.pos + 1),
                    ));
                    indent += 4;
                    self.pos += 1;
                }
                _ => break,
            }
        }

        let Some(byte) = self.byte_at(self.pos) else {
            return;
        };
        if matches!(byte, b'\r' | b'\n' | b'#') {
            return;
        }

        let current = *self.indents.last().expect("indent stack is never empty");
        if indent > current {
            self.indents.push(indent);
            self.push(TokenKind::Indent, Span::new(start, self.pos));
        } else if indent < current {
            while self.indents.len() > 1 && indent < *self.indents.last().unwrap() {
                self.indents.pop();
                self.push(TokenKind::Dedent, Span::new(start, self.pos));
            }
            if indent != *self.indents.last().unwrap() {
                self.diagnostics.push(Diagnostic::error(
                    "NXL005",
                    format!(
                        "inconsistent indentation: expected {} spaces, found {indent}",
                        self.indents.last().unwrap()
                    ),
                    Span::new(start, self.pos),
                ));
            }
        }
        self.at_line_start = false;
    }

    fn consume_newline(&mut self) {
        let start = self.pos;
        if self.byte_at(self.pos) == Some(b'\r') {
            self.pos += 1;
            if self.byte_at(self.pos) == Some(b'\n') {
                self.pos += 1;
            }
        } else {
            self.pos += 1;
        }

        if self.bracket_depth == 0 {
            self.push(TokenKind::Newline, Span::new(start, self.pos));
            self.at_line_start = true;
        }
    }

    fn skip_comment(&mut self) {
        while let Some(byte) = self.byte_at(self.pos) {
            if matches!(byte, b'\r' | b'\n') {
                break;
            }
            self.pos += self.current_char().map(char::len_utf8).unwrap_or(1);
        }
    }

    fn scan_ident(&mut self) {
        let start = self.pos;
        while let Some(byte) = self.byte_at(self.pos) {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.source[start..self.pos];
        let kind = match text {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "var" => TokenKind::Var,
            "const" => TokenKind::Const,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "case" => TokenKind::Case,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "impl" => TokenKind::Impl,
            "trait" => TokenKind::Trait,
            "import" => TokenKind::Import,
            "from" => TokenKind::From,
            "as" => TokenKind::As,
            "pub" => TokenKind::Pub,
            "async" => TokenKind::Async,
            "await" => TokenKind::Await,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "none" => TokenKind::None,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "match" => TokenKind::Match,
            "extern" => TokenKind::Extern,
            "unsafe" => TokenKind::Unsafe,
            "type" => TokenKind::Type,
            "module" => TokenKind::Module,
            "use" => TokenKind::Use,
            _ => TokenKind::Ident(text.to_string()),
        };
        self.push(kind, Span::new(start, self.pos));
    }

    fn scan_number(&mut self) {
        let start = self.pos;

        if self.byte_at(self.pos) == Some(b'0')
            && matches!(self.byte_at(self.pos + 1), Some(b'x' | b'X'))
        {
            self.pos += 2;
            let digits_start = self.pos;
            while matches!(
                self.byte_at(self.pos),
                Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' | b'_')
            ) {
                self.pos += 1;
            }
            if self.pos == digits_start {
                self.diagnostics.push(Diagnostic::error(
                    "NXL008",
                    "hexadecimal literal requires at least one digit",
                    Span::new(start, self.pos),
                ));
                self.push(TokenKind::Int(0), Span::new(start, self.pos));
                return;
            }
            let digits = self.source[digits_start..self.pos].replace('_', "");
            let value = i64::from_str_radix(&digits, 16).unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "NXL009",
                    "integer literal is out of range for i64",
                    Span::new(start, self.pos),
                ));
                0
            });
            self.push(TokenKind::Int(value), Span::new(start, self.pos));
            return;
        }

        if self.byte_at(self.pos) == Some(b'0')
            && matches!(self.byte_at(self.pos + 1), Some(b'b' | b'B'))
        {
            self.pos += 2;
            let digits_start = self.pos;
            while matches!(self.byte_at(self.pos), Some(b'0' | b'1' | b'_')) {
                self.pos += 1;
            }
            if self.pos == digits_start {
                self.diagnostics.push(Diagnostic::error(
                    "NXL008",
                    "binary literal requires at least one digit",
                    Span::new(start, self.pos),
                ));
                self.push(TokenKind::Int(0), Span::new(start, self.pos));
                return;
            }
            let digits = self.source[digits_start..self.pos].replace('_', "");
            let value = i64::from_str_radix(&digits, 2).unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "NXL009",
                    "integer literal is out of range for i64",
                    Span::new(start, self.pos),
                ));
                0
            });
            self.push(TokenKind::Int(value), Span::new(start, self.pos));
            return;
        }

        while matches!(self.byte_at(self.pos), Some(b'0'..=b'9' | b'_')) {
            self.pos += 1;
        }

        let mut is_float = false;
        if self.byte_at(self.pos) == Some(b'.') && self.byte_at(self.pos + 1) != Some(b'.') {
            is_float = true;
            self.pos += 1;
            while matches!(self.byte_at(self.pos), Some(b'0'..=b'9' | b'_')) {
                self.pos += 1;
            }
        }

        if matches!(self.byte_at(self.pos), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.byte_at(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exponent_start = self.pos;
            while matches!(self.byte_at(self.pos), Some(b'0'..=b'9' | b'_')) {
                self.pos += 1;
            }
            if self.pos == exponent_start {
                self.diagnostics.push(Diagnostic::error(
                    "NXL009",
                    "floating-point exponent requires digits",
                    Span::new(start, self.pos),
                ));
            }
        }

        let text = self.source[start..self.pos].replace('_', "");
        if is_float {
            let value = text.parse::<f64>().unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "NXL009",
                    "invalid floating-point literal",
                    Span::new(start, self.pos),
                ));
                0.0
            });
            self.push(TokenKind::Float(value), Span::new(start, self.pos));
        } else {
            let value = text.parse::<i64>().unwrap_or_else(|_| {
                self.diagnostics.push(Diagnostic::error(
                    "NXL009",
                    "integer literal is out of range for i64",
                    Span::new(start, self.pos),
                ));
                0
            });
            self.push(TokenKind::Int(value), Span::new(start, self.pos));
        }
    }

    fn scan_string(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let mut value = String::new();
        let mut terminated = false;

        while let Some(character) = self.current_char() {
            match character {
                '"' => {
                    self.pos += 1;
                    terminated = true;
                    break;
                }
                '\\' => {
                    self.pos += 1;
                    self.scan_escape(&mut value);
                }
                '\n' | '\r' => break,
                _ => {
                    value.push(character);
                    self.pos += character.len_utf8();
                }
            }
        }

        if !terminated {
            self.diagnostics.push(Diagnostic::error(
                "NXL002",
                "unterminated string literal",
                Span::new(start, self.pos),
            ));
        }
        self.push(TokenKind::Str(value), Span::new(start, self.pos));
    }

    fn scan_char(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let mut value = '\0';
        let mut valid = false;

        if let Some(character) = self.current_char() {
            if character == '\'' {
                self.pos += 1;
                self.diagnostics.push(Diagnostic::error(
                    "NXL011",
                    "character literal must contain exactly one character",
                    Span::new(start, self.pos),
                ));
                self.push(TokenKind::Char(value), Span::new(start, self.pos));
                return;
            }

            if character == '\\' {
                self.pos += 1;
                let mut escaped = String::new();
                self.scan_escape(&mut escaped);
                value = escaped.chars().next().unwrap_or('\0');
                valid = !escaped.is_empty();
            } else {
                value = character;
                valid = true;
                self.pos += character.len_utf8();
            }

            if self.byte_at(self.pos) == Some(b'\'') {
                self.pos += 1;
            } else {
                while let Some(character) = self.current_char() {
                    if matches!(character, '\'' | '\n' | '\r') {
                        break;
                    }
                    self.pos += character.len_utf8();
                }
                if self.byte_at(self.pos) == Some(b'\'') {
                    self.pos += 1;
                }
            }
        }

        if !valid {
            self.diagnostics.push(Diagnostic::error(
                "NXL011",
                "character literal must contain exactly one character",
                Span::new(start, self.pos),
            ));
        }
        self.push(TokenKind::Char(value), Span::new(start, self.pos));
    }

    fn scan_escape(&mut self, out: &mut String) {
        let escape_start = self.pos.saturating_sub(1);
        let Some(character) = self.current_char() else {
            self.diagnostics.push(Diagnostic::error(
                "NXL003",
                "unterminated escape sequence",
                Span::new(escape_start, self.pos),
            ));
            return;
        };
        self.pos += character.len_utf8();

        match character {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            '\'' => out.push('\''),
            '0' => {
                self.diagnostics.push(Diagnostic::error(
                    "NXL010",
                    "embedded NUL characters are not supported in NEX 0.1 strings",
                    Span::new(escape_start, self.pos),
                ));
            }
            'x' => {
                let byte_start = self.pos;
                let mut digits = String::new();
                for _ in 0..2 {
                    if let Some(byte) = self.byte_at(self.pos) {
                        if byte.is_ascii_hexdigit() {
                            digits.push(byte as char);
                            self.pos += 1;
                        }
                    }
                }
                if digits.len() != 2 {
                    self.diagnostics.push(Diagnostic::error(
                        "NXL003",
                        "`\\x` escape requires exactly two hexadecimal digits",
                        Span::new(escape_start, self.pos),
                    ));
                } else {
                    let byte = u8::from_str_radix(&digits, 16).unwrap_or(0);
                    if byte <= 0x7F {
                        out.push(byte as char);
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "NXL003",
                            "non-ASCII `\\x` escapes are not supported; use `\\u{...}`",
                            Span::new(byte_start, self.pos),
                        ));
                    }
                }
            }
            'u' => {
                if self.byte_at(self.pos) != Some(b'{') {
                    self.diagnostics.push(Diagnostic::error(
                        "NXL003",
                        "`\\u` escape requires braces, for example `\\u{1F600}`",
                        Span::new(escape_start, self.pos),
                    ));
                    return;
                }
                self.pos += 1;
                let digits_start = self.pos;
                let mut digits = String::new();
                while let Some(byte) = self.byte_at(self.pos) {
                    if byte == b'}' || !byte.is_ascii_hexdigit() || digits.len() == 6 {
                        break;
                    }
                    digits.push(byte as char);
                    self.pos += 1;
                }
                if self.byte_at(self.pos) == Some(b'}') {
                    self.pos += 1;
                }
                let code = u32::from_str_radix(&digits, 16).ok();
                match code.and_then(char::from_u32) {
                    Some(character) => out.push(character),
                    None => self.diagnostics.push(Diagnostic::error(
                        "NXL003",
                        "invalid Unicode escape",
                        Span::new(digits_start, self.pos),
                    )),
                }
            }
            other => {
                self.diagnostics.push(Diagnostic::error(
                    "NXL003",
                    format!("unknown escape sequence `\\{other}`"),
                    Span::new(escape_start, self.pos),
                ));
                out.push(other);
            }
        }
    }

    fn scan_operator(&mut self) {
        let start = self.pos;
        let rest = &self.source[self.pos..];

        let (kind, length) = if rest.starts_with("...") {
            (TokenKind::DotDotDot, 3)
        } else if rest.starts_with("..") {
            (TokenKind::DotDot, 2)
        } else if rest.starts_with("->") {
            (TokenKind::Arrow, 2)
        } else if rest.starts_with("+=") {
            (TokenKind::PlusEq, 2)
        } else if rest.starts_with("-=") {
            (TokenKind::MinusEq, 2)
        } else if rest.starts_with("*=") {
            (TokenKind::StarEq, 2)
        } else if rest.starts_with("/=") {
            (TokenKind::SlashEq, 2)
        } else if rest.starts_with("%=") {
            (TokenKind::PercentEq, 2)
        } else if rest.starts_with("==") {
            (TokenKind::EqEq, 2)
        } else if rest.starts_with("!=") {
            (TokenKind::BangEq, 2)
        } else if rest.starts_with("<=") {
            (TokenKind::LtEq, 2)
        } else if rest.starts_with(">=") {
            (TokenKind::GtEq, 2)
        } else if rest.starts_with("<<") {
            (TokenKind::Shl, 2)
        } else if rest.starts_with(">>") {
            (TokenKind::Shr, 2)
        } else if rest.starts_with("&&") {
            (TokenKind::AmpAmp, 2)
        } else if rest.starts_with("||") {
            (TokenKind::PipePipe, 2)
        } else {
            match rest.chars().next() {
                Some('(') => (TokenKind::LParen, 1),
                Some(')') => (TokenKind::RParen, 1),
                Some('[') => (TokenKind::LBracket, 1),
                Some(']') => (TokenKind::RBracket, 1),
                Some('{') => (TokenKind::LBrace, 1),
                Some('}') => (TokenKind::RBrace, 1),
                Some(',') => (TokenKind::Comma, 1),
                Some(':') => (TokenKind::Colon, 1),
                Some(';') => (TokenKind::Semicolon, 1),
                Some('.') => (TokenKind::Dot, 1),
                Some('?') => (TokenKind::Question, 1),
                Some('+') => (TokenKind::Plus, 1),
                Some('-') => (TokenKind::Minus, 1),
                Some('*') => (TokenKind::Star, 1),
                Some('/') => (TokenKind::Slash, 1),
                Some('%') => (TokenKind::Percent, 1),
                Some('=') => (TokenKind::Eq, 1),
                Some('!') => (TokenKind::Bang, 1),
                Some('<') => (TokenKind::Lt, 1),
                Some('>') => (TokenKind::Gt, 1),
                Some('&') => (TokenKind::Amp, 1),
                Some('|') => (TokenKind::Pipe, 1),
                Some('^') => (TokenKind::Caret, 1),
                Some('~') => (TokenKind::Tilde, 1),
                Some(character) => {
                    let span = Span::new(start, start + character.len_utf8());
                    if character.is_alphabetic() {
                        self.diagnostics.push(Diagnostic::error(
                            "NXL007",
                            "non-ASCII identifiers are not supported yet",
                            span,
                        ));
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            "NXL001",
                            format!("unexpected character `{character}`"),
                            span,
                        ));
                    }
                    self.pos += character.len_utf8();
                    return;
                }
                None => return,
            }
        };

        self.pos += length;

        match kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                self.bracket_depth += 1;
            }
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
            }
            _ => {}
        }

        self.push(kind, Span::new(start, self.pos));
    }
}
