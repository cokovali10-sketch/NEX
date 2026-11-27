//! Diagnostics are a first-class part of the NEX compiler.
//!
//! Every token, AST node and error carries a byte [`Span`]. The driver keeps the
//! original [`SourceFile`] so diagnostics can be rendered with file, line,
//! column and a caret without leaking source text into the compiler stages.

use std::fmt::Write as _;

/// A half-open byte range in a UTF-8 source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn empty(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start >= self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

/// A single compiler message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub span: Option<Span>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            span: Some(span),
            notes: Vec::new(),
        }
    }

    pub fn error_without_span(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            span: None,
            notes: Vec::new(),
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.into(),
            message: message.into(),
            span: Some(span),
            notes: Vec::new(),
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Ordered collection of diagnostics produced by one compilation stage.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticBag {
    items: Vec<Diagnostic>,
}

impl DiagnosticBag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(diagnostics);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(Diagnostic::is_error)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}

/// Source text plus a line index for O(log n) line/column lookup.
#[derive(Debug, Clone)]
pub struct SourceFile {
    name: String,
    text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            name: name.into(),
            text,
            line_starts,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// One-based line and one-based character column.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line_index = self.line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        let column = self.text[line_start..offset].chars().count() + 1;
        (line_index + 1, column)
    }

    /// One-based line text, without the trailing newline.
    pub fn line_text(&self, line: usize) -> &str {
        if line == 0 || line > self.line_starts.len() {
            return "";
        }
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .copied()
            .unwrap_or(self.text.len());
        self.text[start..end]
            .trim_end_matches('\n')
            .trim_end_matches('\r')
    }
}

/// Render one diagnostic in a rustc-inspired, terminal-friendly layout.
pub fn render(source: &SourceFile, diagnostic: &Diagnostic) -> String {
    let mut out = String::new();
    let code = if diagnostic.code.is_empty() {
        String::new()
    } else {
        format!("[{}]", diagnostic.code)
    };
    let _ = writeln!(
        out,
        "{}{}: {}",
        diagnostic.severity.label(),
        code,
        diagnostic.message
    );

    if let Some(span) = diagnostic.span {
        // Spans from imported modules are offset by the driver and can exceed
        // this source's length; clamp so rendering never panics.
        let text_len = source.text.len();
        let span_start = span.start.min(text_len);
        let (line, column) = source.line_col(span_start);
        let line_text = source.line_text(line);
        let _ = writeln!(out, " --> {}:{}:{}", source.name(), line, column);
        let _ = writeln!(out, "  |");
        let _ = writeln!(out, "{line:>3} | {line_text}");
        let line_start = source
            .line_starts
            .get(line.saturating_sub(1))
            .copied()
            .unwrap_or(0);
        let line_end = line_start + line_text.len();
        let highlight_end = span.end.min(line_end).max(span_start).min(text_len);
        let width = source.text[span_start.min(highlight_end)..highlight_end]
            .chars()
            .count()
            .max(1);
        let caret_padding = " ".repeat(column.saturating_sub(1));
        let _ = writeln!(out, "  | {caret_padding}{}", "^".repeat(width.max(1)));
    }

    for note in &diagnostic.notes {
        let _ = writeln!(out, "  = note: {note}");
    }
    if let Some(help) = help_for_code(&diagnostic.code) {
        let _ = writeln!(out, "  = help: {help}");
    }
    out
}

/// Return a short, actionable suggestion for a known diagnostic code.
pub fn help_for_code(code: &str) -> Option<&'static str> {
    match code {
        "NXL004" => Some("use four spaces for each indentation level; tabs are not allowed"),
        "NXP004" => Some("call a declared function or struct constructor by name"),
        "NXL002" => Some("close the string with a matching quote on this line"),
        "NXL003" => Some("use a supported escape such as `\\n`, `\\t`, `\\\\`, or `\\u{1F600}`"),
        "NXL005" => Some("make indentation use consistent groups of four spaces"),
        "NXS001" => {
            Some("check the spelling, declare the name first, or import it from its module")
        }
        "NXS012" => Some("use `break` or `continue` inside a `for` or `while` loop"),
        "NXS014" => Some("make the condition a boolean expression, for example `count > 0`"),
        "NXS044" => Some(
            "make the implementation method's receiver, parameters, and return type match the trait",
        ),
        "NXS045" => Some("add every method required by the trait implementation"),
        "NXS046" => {
            Some("check the receiver type and method name, or call a declared function directly")
        }
        "NXS047" => Some(
            "store the nested field in a local variable, then call the method on that variable",
        ),
        "NXS051" => {
            Some("use a mutable reference to assign through a field, index, or dereference")
        }
        "NXS052" => Some("borrow a variable or another assignable place, such as `&value`"),
        "NXP010" => Some("declare a function, constant, struct, enum, or module at this level"),
        "NXS002" => Some(
            "use a concrete value type such as `i64`, `f64`, `bool`, `char`, `string`, or a declared struct/enum",
        ),
        "NXS003" => Some("choose a unique name for this declaration"),
        "NXS004" => Some("check the function name, argument count, and argument types"),
        "NXS006" => Some("convert the value explicitly with `as` or change the expected type"),
        "NXS005" => Some("make the number of bound names match the tuple or struct fields"),
        "NXS007" => Some("declare the binding with `var` when it must be mutated"),
        "NXS008" => Some("make both operands the same type; NEX does not apply implicit widening"),
        "NXS009" => Some("remove the returned value or declare a return type"),
        "NXS010" => {
            Some("add a `return` on every control-flow path, or change the return type to `void`")
        }
        "NXS011" => Some("`main` must take no parameters and return `void` or `i32`"),
        "NXS015" => {
            Some("print scalar fields individually, or print an aggregate value as a whole")
        }
        "NXS016" => Some("declare the variable before assigning to its field or element"),
        "NXS017" => Some("remove the unreachable statement or restructure the control flow"),
        "NXS018" => Some("rename the duplicate parameter"),
        "NXS019" => Some("choose a different function name; `print` and `println` are built in"),
        "NXS020" => Some("avoid the reserved `nex_rt_` prefix"),
        "NXS021" => Some("use a literal that fits in the target integer type"),
        "NXS023" => Some("initialize the constant with a compile-time constant expression"),
        "NXS025" => {
            Some("ensure the struct has at least one field and avoid recursive field types")
        }
        "NXS027" => Some("check the field or element name and the value's type"),
        "NXS028" => Some("use a scalar type for this constant"),
        "NXS029" => Some("declare at least one variant in the enum"),
        "NXS030" => Some("check the enum name and variant name"),
        "NXS033" => Some("add the missing `case` arms or a trailing `else:` arm"),
        "NXS035" => Some("provide at least one array element or type argument"),
        "NXS036" => Some("use an integer expression as the array index"),
        "NXS037" => Some("provide at least one tuple element"),
        "NXS038" => Some("declare the item with `pub` if it should be visible outside its module"),
        "NXC001" => Some("check the generated IR by running `nex check` and fixing earlier errors"),
        "NXC002" => Some("install Clang/LLVM and run `nex doctor`"),
        _ => None,
    }
}

pub fn render_all(source: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&render(source, diagnostic));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column_are_one_based() {
        let source = SourceFile::new("test.nex", "fn main():\n    print(\"hi\")\n");
        assert_eq!(source.line_col(0), (1, 1));
        assert_eq!(source.line_col(11), (2, 1));
        assert_eq!(source.line_col(15), (2, 5));
    }

    #[test]
    fn renders_span_with_caret() {
        let source = SourceFile::new("test.nex", "fn main():\n    let x = 1\n");
        let diagnostic = Diagnostic::error("NXS001", "unknown variable `x`", Span::new(15, 16));
        let rendered = render(&source, &diagnostic);
        assert!(rendered.contains("error[NXS001]: unknown variable `x`"));
        assert!(rendered.contains("test.nex:2:5"));
        assert!(rendered.contains("^"));
    }

    #[test]
    fn common_errors_have_actionable_help() {
        assert!(
            help_for_code("NXS001")
                .unwrap()
                .contains("declare the name")
        );
        assert!(help_for_code("NXS046").unwrap().contains("method name"));
        assert!(help_for_code("NXS999").is_none());
    }
    #[test]
    fn spans_merge_to_cover_both_sides() {
        assert_eq!(Span::new(2, 4).merge(Span::new(6, 9)), Span::new(2, 9));
    }
}
