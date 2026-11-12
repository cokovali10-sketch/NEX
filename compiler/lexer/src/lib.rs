//! Indentation-aware lexer for NEX.
//!
//! NEX uses significant indentation (four spaces per level). Tabs in leading
//! indentation are rejected: silent tab/space mixing is a known source of
//! layout bugs, so the language does not allow it.

mod lexer;
mod token;

pub use lexer::{LexResult, Lexer};
pub use token::{Token, TokenKind};

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> LexResult {
        Lexer::new(source).lex()
    }

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .tokens
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_hello_world() {
        let result = lex("fn main():\n    print(\"Hello from NEX!\")\n");
        assert!(result.diagnostics.is_empty());
        let kinds = result
            .tokens
            .iter()
            .map(|token| token.kind.clone())
            .collect::<Vec<_>>();
        assert!(kinds.contains(&TokenKind::Fn));
        assert!(kinds.contains(&TokenKind::Indent));
        assert!(kinds.contains(&TokenKind::Dedent));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, TokenKind::Str(s) if s == "Hello from NEX!"))
        );
    }

    #[test]
    fn lexes_operators_and_comments() {
        let result = lex("let x = 1 + 2 * 3 # comment\n");
        assert!(result.diagnostics.is_empty());
        assert!(kinds("let x = 1 + 2 * 3 # comment\n").contains(&TokenKind::Star));
    }

    #[test]
    fn tracks_indentation_levels() {
        let source = "fn main():\n    if true:\n        print(\"x\")\n    print(\"y\")\n";
        let result = lex(source);
        assert!(result.diagnostics.is_empty());
        let indents = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Indent)
            .count();
        let dedents = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Dedent)
            .count();
        assert_eq!(indents, 2);
        assert_eq!(dedents, 2);
    }

    #[test]
    fn ignores_blank_lines_for_indentation() {
        let source = "fn main():\n\n    print(\"x\")\n\nprint(\"done\")\n";
        let result = lex(source);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn scans_numbers() {
        let kinds = kinds("0 42 1_000 0xFF 0b1010 3.5 1.0e-3");
        assert!(kinds.contains(&TokenKind::Int(42)));
        assert!(kinds.contains(&TokenKind::Int(1000)));
        assert!(kinds.contains(&TokenKind::Int(255)));
        assert!(kinds.contains(&TokenKind::Int(10)));
        assert!(kinds.iter().any(
            |kind| matches!(kind, TokenKind::Float(value) if (*value - 3.5).abs() < f64::EPSILON)
        ));
        assert!(
            kinds.iter().any(
                |kind| matches!(kind, TokenKind::Float(value) if (*value - 0.001).abs() < 1e-12)
            )
        );
    }

    #[test]
    fn scans_string_escapes() {
        let result = lex("\"line\\nquote\\\"tab\\t\"");
        assert!(result.diagnostics.is_empty());
        assert!(result.tokens.iter().any(
            |token| matches!(&token.kind, TokenKind::Str(value) if value == "line\nquote\"tab\t")
        ));
    }

    #[test]
    fn scans_char_literals() {
        let result = lex("'a' '\\n' '\\u{1F600}'");
        assert!(result.diagnostics.is_empty());
        let chars = result
            .tokens
            .iter()
            .filter_map(|token| match token.kind {
                TokenKind::Char(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(chars, vec!['a', '\n', '\u{1F600}']);
    }

    #[test]
    fn rejects_tabs_in_indentation() {
        let result = lex("fn main():\n\tprint(\"x\")\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXL004"));
    }

    #[test]
    fn rejects_unterminated_string() {
        let result = lex("\"hello\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXL002"));
    }

    #[test]
    fn rejects_inconsistent_dedent() {
        let result = lex("fn main():\n    if true:\n        print(\"x\")\n  print(\"y\")\n");
        assert!(result.diagnostics.iter().any(|d| d.code == "NXL005"));
    }

    #[test]
    fn inserts_final_newline_at_eof() {
        let result = lex("fn main():\n    print(\"x\")");
        assert!(result.diagnostics.is_empty());
        assert!(
            result
                .tokens
                .iter()
                .rev()
                .nth(2)
                .is_some_and(|token| token.kind == TokenKind::Newline)
        );
    }

    #[test]
    fn newlines_inside_parentheses_are_ignored() {
        let result = lex("let x = (1 +\n  2)\n");
        assert!(result.diagnostics.is_empty());
        let newlines = result
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Newline)
            .count();
        assert_eq!(newlines, 1);
    }
}
