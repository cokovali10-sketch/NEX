//! Minimal Language Server Protocol (LSP) front-end for the `nex` CLI.
//!
//! The server speaks JSON-RPC 2.0 over stdio using the LSP framing
//! (`Content-Length: N\r\n\r\n{json}`). It exposes hover, go-to-definition,
//! completion, document symbols, live diagnostics and formatting for NEX
//! source files. No external language tooling is required; NEX files are
//! re-analyzed with the compiler that ships with the CLI.

use nex_ast::TypeExpr;
use nex_diagnostics::{Diagnostic, Severity, SourceFile, Span};
use nex_fmt::format_source;
use nex_lexer::Lexer;
use nex_parser::parse;
use nex_semantic::{Analysis, analyze};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufWriter, Write};
use std::process::ExitCode;

const SERVER_NAME: &str = "nex-lsp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone)]
pub struct Document {
    pub uri: String,
    pub version: u32,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct LspState {
    pub documents: HashMap<String, Document>,
}

/// Lifecycle status of the server. `exit` is only accepted once `shutdown` ran.
#[derive(Debug)]
struct Server {
    state: LspState,
    writer: BufWriter<io::StdoutLock<'static>>,
    shutdown: bool,
}

enum Incoming {
    Request {
        id: u64,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

pub fn run() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut server = Server {
        state: LspState::default(),
        writer: BufWriter::new(stdout.lock()),
        shutdown: false,
    };

    while let Some(message) = read_message(stdin.lock()) {
        match message {
            Incoming::Request { id, method, params } => {
                let response = server.handle_request(id, &method, &params);
                if !server.write_response(&response) {
                    break;
                }
            }
            Incoming::Notification { method, params } => match method.as_str() {
                "exit" => return ExitCode::SUCCESS,
                _ => {
                    if server.handle_notification(&method, &params).is_none() {
                        break;
                    }
                }
            },
        }
    }
    ExitCode::SUCCESS
}

fn read_message(mut reader: impl BufRead) -> Option<Incoming> {
    let mut content_length = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map(|n| n == 0).unwrap_or(true) {
            return None;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().to_string();
        }
    }
    let length: usize = content_length.parse().ok()?;
    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer).ok()?;
    let body = String::from_utf8_lossy(&buffer).to_string();
    let body = body.trim_end_matches(['\r', '\n']);
    let value: Value = serde_json::from_str(body).ok()?;
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let params = value.get("params").cloned().unwrap_or(Value::Null);
    if let Some(id) = value.get("id").and_then(Value::as_u64) {
        Some(Incoming::Request { id, method, params })
    } else {
        Some(Incoming::Notification { method, params })
    }
}

impl Server {
    fn write_response(&mut self, response: &Value) -> bool {
        let body = response.to_string();
        self.writer
            .write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
            .and_then(|()| self.writer.flush())
            .is_ok()
    }

    fn send_notification(&mut self, method: &str, params: Value) -> bool {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_response(&notification)
    }

    fn handle_request(&mut self, id: u64, method: &str, params: &Value) -> Value {
        let result = match method {
            "initialize" => self.handle_initialize(params),
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "textDocument/hover" => self.with_document(params, handle_hover),
            "textDocument/definition" => self.with_document(params, handle_definition),
            "textDocument/completion" => self.with_document(params, handle_completion),
            "textDocument/documentSymbol" => self.with_document(params, handle_document_symbols),
            "textDocument/formatting" => self.with_document(params, handle_formatting),
            _ => Err(json!({
                "code": -32601,
                "message": format!("method not supported: {method}"),
            })),
        };

        match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        }
    }

    fn handle_notification(&mut self, method: &str, params: &Value) -> Option<()> {
        match method {
            "initialized" => {}
            "textDocument/didOpen" => {
                let document = &params.get("textDocument")?;
                let uri = text_document_uri(params)?;
                let version = document.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
                let text = document
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                self.state
                    .documents
                    .insert(uri.clone(), Document { uri, version, text });
            }
            "textDocument/didChange" => {
                let uri = text_document_uri(params)?;
                let changes = params.get("contentChanges").and_then(Value::as_array)?;
                for change in changes {
                    let document = self.state.documents.get_mut(&uri)?;
                    let new_text = change.get("text").and_then(Value::as_str)?;
                    if let Some(range) = change.get("range") {
                        let start = range.get("start").and_then(position_of_value)?;
                        let end = range.get("end").and_then(position_of_value)?;
                        let start_offset = position_to_offset(&document.text, start.0, start.1);
                        let end_offset = position_to_offset(&document.text, end.0, end.1);
                        document.text = format!(
                            "{}{}{}",
                            &document.text[..start_offset],
                            new_text,
                            &document.text[end_offset..]
                        );
                    } else {
                        document.text = new_text.to_string();
                    }
                    if let Some(version) = change.get("version").and_then(Value::as_u64) {
                        document.version = version as u32;
                    }
                }
            }
            "textDocument/didSave" => {
                let uri = text_document_uri(params)?;
                if let Some(text) = params.get("text").and_then(Value::as_str) {
                    if let Some(document) = self.state.documents.get_mut(&uri) {
                        document.text = text.to_string();
                    }
                }
            }
            _ => return Some(()),
        }
        let uri = text_document_uri(params);
        if let Some(uri) = uri {
            self.publish_diagnostics(&uri);
        }
        Some(())
    }

    fn publish_diagnostics(&mut self, uri: &str) {
        let Some(document) = self.state.documents.get(uri) else {
            return;
        };
        let (diagnostics, source) = diagnostics_for(uri, &document.text);
        let diagnostics: Vec<Value> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic_to_lsp(diagnostic, &source))
            .collect();
        let _ = self.send_notification(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "diagnostics": diagnostics }),
        );
    }

    fn handle_initialize(&mut self, _params: &Value) -> Result<Value, Value> {
        Ok(json!({
            "capabilities": {
                "textDocumentSync": 1,
                "hoverProvider": true,
                "definitionProvider": true,
                "completionProvider": {
                    "triggerCharacters": ["."],
                    "resolveProvider": false,
                },
                "documentFormattingProvider": true,
                "documentSymbolProvider": true,
                "referencesProvider": false,
                "renameProvider": false,
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            },
        }))
    }

    fn with_document<F>(&mut self, params: &Value, handler: F) -> Result<Value, Value>
    where
        F: FnOnce(&Document, &Value) -> Result<Value, Value>,
    {
        let uri = text_document_uri(params)
            .ok_or_else(|| json!({ "code": -32602, "message": "missing textDocument.uri" }))?;
        let document = self
            .state
            .documents
            .get(&uri)
            .ok_or_else(|| json!({ "code": -32602, "message": "document not open: {uri}" }))?;
        handler(document, params)
    }
}

const KEYWORDS: &[&str] = &[
    "fn", "pub", "let", "var", "const", "type", "use", "from", "extern", "as", "self", "struct",
    "enum", "trait", "impl", "if", "else", "elif", "while", "for", "in", "return", "pass", "break",
    "continue", "match", "case", "defer", "unsafe",
];

const BUILTINS: &[(&str, &str, &str)] = &[
    (
        "print",
        "fn print(value)",
        "Write a value to standard output without a trailing newline.",
    ),
    (
        "println",
        "fn println(value)",
        "Write a value to standard output followed by a newline.",
    ),
    (
        "len",
        "fn len(value) -> i64",
        "Return the number of elements in an array, or the length of a string.",
    ),
    (
        "push",
        "fn push(array, value)",
        "Append an element to a growable array.",
    ),
    (
        "pop",
        "fn pop(array) -> T",
        "Remove and return the last element of an array (LIFO).",
    ),
    (
        "contains",
        "fn contains(array, value) -> bool",
        "Return `true` when the array holds `value`.",
    ),
    (
        "index_of",
        "fn index_of(array, value) -> i64",
        "Return the index of `value` in the array, or -1 when absent.",
    ),
    (
        "reverse",
        "fn reverse(array)",
        "Reverse the elements of an array in place.",
    ),
    (
        "concat",
        "fn concat(left, right) -> string",
        "Concatenate two strings.",
    ),
    (
        "to_string",
        "fn to_string(value) -> string",
        "Render a value as a string.",
    ),
    (
        "str_substring",
        "fn str_substring(value, start, end) -> string",
        "Return the substring covering `[start, end)`.",
    ),
    (
        "str_contains",
        "fn str_contains(value, needle) -> bool",
        "Return `true` when `value` contains `needle`.",
    ),
    (
        "str_index_of",
        "fn str_index_of(value, needle) -> i64",
        "Return the index of `needle` in `value`, or -1.",
    ),
    (
        "str_upper",
        "fn str_upper(value) -> string",
        "Return `value` in uppercase.",
    ),
    (
        "str_lower",
        "fn str_lower(value) -> string",
        "Return `value` in lowercase.",
    ),
    (
        "str_trim",
        "fn str_trim(value) -> string",
        "Strip leading and trailing whitespace.",
    ),
    (
        "floor",
        "fn floor(x: f64) -> f64",
        "Return the largest integer value not greater than `x`.",
    ),
    (
        "ceil",
        "fn ceil(x: f64) -> f64",
        "Return the smallest integer value greater than or equal to `x`.",
    ),
    (
        "round",
        "fn round(x: f64) -> f64",
        "Return the nearest integer to `x`.",
    ),
    (
        "trunc",
        "fn trunc(x: f64) -> f64",
        "Return the integer part of `x` toward zero.",
    ),
    ("abs", "fn abs(x) -> T", "Return the absolute value."),
    (
        "min",
        "fn min(a, b) -> T",
        "Return the smaller of two values.",
    ),
    (
        "max",
        "fn max(a, b) -> T",
        "Return the larger of two values.",
    ),
    ("sqrt", "fn sqrt(x: f64) -> f64", "Return the square root."),
    (
        "pow",
        "fn pow(x: f64, y: f64) -> f64",
        "Return `x` raised to the power `y`.",
    ),
    (
        "assert",
        "fn assert(cond: bool, message: string)",
        "Abort with `message` when `cond` is false.",
    ),
    (
        "panic",
        "fn panic(message: string)",
        "Abort the process with `message`.",
    ),
    (
        "sleep_ms",
        "fn sleep_ms(ms: i64)",
        "Sleep for `ms` milliseconds.",
    ),
    (
        "unix_time",
        "fn unix_time() -> i64",
        "Return the current Unix timestamp in seconds.",
    ),
    (
        "now_ms",
        "fn now_ms() -> i64",
        "Return the current Unix timestamp in milliseconds.",
    ),
    (
        "read_file",
        "fn read_file(path: string) -> string?",
        "Read a whole file into a string.",
    ),
    (
        "file_exists",
        "fn file_exists(path: string) -> bool",
        "Return true when the file exists.",
    ),
    (
        "file_size",
        "fn file_size(path: string) -> i64",
        "Return the size of a file in bytes.",
    ),
    (
        "json_stringify",
        "fn json_stringify(value) -> string",
        "Serialize a value to a JSON string.",
    ),
    ("alloc", "fn alloc(size: i64) -> i64", "Allocate raw bytes."),
    ("free", "fn free(ptr: i64)", "Free a raw allocation."),
    (
        "peek",
        "fn peek(ptr: i64) -> i64",
        "Read a raw 8-byte value.",
    ),
    (
        "poke",
        "fn poke(ptr: i64, value: i64)",
        "Write a raw 8-byte value.",
    ),
    (
        "thread_spawn",
        "fn thread_spawn(fn) -> i64",
        "Spawn a runtime thread.",
    ),
    (
        "thread_join",
        "fn thread_join(id: i64)",
        "Wait for a thread to finish.",
    ),
    (
        "tcp_connect",
        "fn tcp_connect(host, port) -> i64",
        "Open a TCP connection.",
    ),
    (
        "tcp_listen",
        "fn tcp_listen(port) -> i64",
        "Listen on a TCP port.",
    ),
    (
        "http_response",
        "fn http_response(status, body) -> string",
        "Build an HTTP response.",
    ),
];

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn offset_to_position(text: &str, offset: usize) -> (u32, u32) {
    let starts = line_starts(text);
    let offset = offset.min(text.len());
    let line_index = match starts.binary_search(&offset) {
        Ok(index) => index,
        Err(0) => 0,
        Err(insert) => insert - 1,
    };
    let line_start = starts[line_index];
    let mut character: u32 = 0;
    for ch in text[line_start..offset].chars() {
        character += ch.len_utf16() as u32;
    }
    (line_index as u32, character)
}

fn position_to_offset(text: &str, line: u32, character: u32) -> usize {
    let starts = line_starts(text);
    let line_index = (line as usize).min(starts.len() - 1);
    let line_start = starts[line_index];
    let line_end = starts.get(line_index + 1).copied().unwrap_or(text.len());
    let mut offset = line_start;
    let mut remaining = character;
    for ch in text[line_start..line_end].chars() {
        if ch == '\n' {
            break;
        }
        let width = ch.len_utf16() as u32;
        if width > remaining {
            break;
        }
        remaining = remaining.saturating_sub(width);
        offset += ch.len_utf8();
    }
    offset
}

fn text_document_uri(params: &Value) -> Option<String> {
    params
        .get("textDocument")
        .and_then(|doc| doc.get("uri"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn position_of(params: &Value) -> Option<(u32, u32)> {
    position_of_value(params.get("position")?)
}

fn position_of_value(value: &Value) -> Option<(u32, u32)> {
    let line = value.get("line").and_then(Value::as_u64)? as u32;
    let character = value.get("character").and_then(Value::as_u64)? as u32;
    Some((line, character))
}

fn identifier_at(text: &str, offset: usize) -> Option<(usize, usize, String)> {
    let mut start = offset.min(text.len());
    while start > 0 {
        let ch = text[..start].chars().next_back()?;
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            break;
        }
        start -= ch.len_utf8();
    }
    let mut end = offset.min(text.len());
    while end < text.len() {
        let ch = text[end..].chars().next()?;
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            break;
        }
        end += ch.len_utf8();
    }
    if start == end {
        return None;
    }
    Some((start, end, text[start..end].to_string()))
}

fn is_method_access(text: &str, start: usize) -> bool {
    if start == 0 {
        return false;
    }
    text[..start].trim_end().ends_with('.')
}

fn current_word_prefix(text: &str, offset: usize) -> String {
    let mut start = offset.min(text.len());
    while start > 0 {
        let ch = text[..start].chars().next_back();
        match ch {
            Some(c) if c.is_ascii_alphanumeric() || c == '_' => start -= c.len_utf8(),
            _ => break,
        }
    }
    text[start..offset].to_string()
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
    ty.name.clone()
}

fn fn_detail(name: &str, params: &[nex_ast::Param], ret: Option<&TypeExpr>) -> String {
    let rendered: Vec<String> = params
        .iter()
        .map(|param| format!("{}: {}", param.name, type_str(&param.ty)))
        .collect();
    let mut detail = format!("fn {name}({})", rendered.join(", "));
    if let Some(ret) = ret {
        detail.push_str(&format!(" -> {}", type_str(ret)));
    }
    detail
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    Keyword,
    Builtin,
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Const,
    Global,
    Alias,
}

#[derive(Debug, Clone)]
struct SymbolInfo {
    kind: SymbolKind,
    name: String,
    detail: String,
    documentation: String,
    span: Option<Span>,
}

fn collect_symbols(program: &nex_ast::Program, analysis: &Analysis) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    for function in &program.functions {
        symbols.push(SymbolInfo {
            kind: SymbolKind::Function,
            name: function.name.clone(),
            detail: fn_detail(&function.name, &function.params, function.ret.as_ref()),
            documentation: "function declared in this file".to_string(),
            span: Some(function.span),
        });
    }
    for block in &program.impls {
        for method in &block.methods {
            let receiver = block.type_name.clone();
            let receiver_name = format!("{receiver}.{}", method.name);
            let detail = analysis
                .functions
                .get(&receiver_name)
                .map(|sig| {
                    let params: Vec<String> =
                        sig.params.iter().skip(1).map(|ty| ty.to_string()).collect();
                    let mut text = format!("fn {}({})", method.name, params.join(", "));
                    text.push_str(&format!(" -> {}", sig.ret));
                    text.push_str(&format!(
                        "\n// impl {} in {}",
                        block.trait_name, block.type_name
                    ));
                    text
                })
                .unwrap_or_else(|| fn_detail(&method.name, &method.params, method.ret.as_ref()));
            symbols.push(SymbolInfo {
                kind: SymbolKind::Method,
                name: method.name.clone(),
                detail,
                documentation: format!("method of `{receiver}` declared in this file",),
                span: Some(method.span),
            });
        }
    }
    for definition in &program.structs {
        let fields: Vec<String> = definition
            .fields
            .iter()
            .map(|field| format!("{}: {}", field.name, type_str(&field.ty)))
            .collect();
        symbols.push(SymbolInfo {
            kind: SymbolKind::Struct,
            name: definition.name.clone(),
            detail: format!("struct {}", definition.name),
            documentation: format!(
                "struct with fields:\n{}",
                fields
                    .iter()
                    .map(|field| format!("- {field}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            span: Some(definition.span),
        });
    }
    for definition in &program.enums {
        let variants: Vec<String> = definition
            .variants
            .iter()
            .map(|variant| variant.name.clone())
            .collect();
        symbols.push(SymbolInfo {
            kind: SymbolKind::Enum,
            name: definition.name.clone(),
            detail: format!("enum {}", definition.name),
            documentation: format!("enum with variants:\n- {}", variants.join("\n- ")),
            span: Some(definition.span),
        });
    }
    for definition in &program.traits {
        let methods: Vec<String> = definition
            .methods
            .iter()
            .map(|method| method.name.clone())
            .collect();
        symbols.push(SymbolInfo {
            kind: SymbolKind::Trait,
            name: definition.name.clone(),
            detail: format!("trait {}", definition.name),
            documentation: format!("trait with methods: {}", methods.join(", ")),
            span: Some(definition.span),
        });
    }
    for constant in &program.consts {
        let ty = constant
            .ty
            .as_ref()
            .map(type_str)
            .unwrap_or_else(|| "<inferred>".to_string());
        symbols.push(SymbolInfo {
            kind: SymbolKind::Const,
            name: constant.name.clone(),
            detail: format!("const {}: {ty}", constant.name),
            documentation: "constant declared in this file".to_string(),
            span: Some(constant.span),
        });
    }
    for global in &program.globals {
        let ty = global
            .ty
            .as_ref()
            .map(type_str)
            .unwrap_or_else(|| "<inferred>".to_string());
        symbols.push(SymbolInfo {
            kind: SymbolKind::Global,
            name: global.name.clone(),
            detail: format!("var {}: {ty}", global.name),
            documentation: "module-level variable declared in this file".to_string(),
            span: Some(global.span),
        });
    }
    for alias in &program.type_aliases {
        symbols.push(SymbolInfo {
            kind: SymbolKind::Alias,
            name: alias.name.clone(),
            detail: format!("type {} = {}", alias.name, type_str(&alias.ty)),
            documentation: "type alias declared in this file".to_string(),
            span: Some(alias.span),
        });
    }
    symbols
}

fn builtin_symbol(name: &str) -> Option<SymbolInfo> {
    BUILTINS
        .iter()
        .find_map(|(builtin, detail, documentation)| {
            if *builtin == name {
                Some(SymbolInfo {
                    kind: SymbolKind::Builtin,
                    name: name.to_string(),
                    detail: (*detail).to_string(),
                    documentation: (*documentation).to_string(),
                    span: None,
                })
            } else {
                None
            }
        })
}

fn keyword_symbol(name: &str) -> Option<SymbolInfo> {
    if KEYWORDS.contains(&name) {
        Some(SymbolInfo {
            kind: SymbolKind::Keyword,
            name: name.to_string(),
            detail: format!("keyword `{name}`"),
            documentation: keyword_documentation(name),
            span: None,
        })
    } else {
        None
    }
}

fn keyword_documentation(name: &str) -> String {
    let description = match name {
        "fn" => "Declare a function.",
        "pub" => "Make a declaration visible outside its module.",
        "let" => "Declare an immutable local binding.",
        "var" => "Declare a mutable local or module-level binding.",
        "const" => "Declare a compile-time constant.",
        "type" => "Declare a type alias.",
        "use" => "Import items from a module.",
        "from" => "Introduce the file path a module is loaded from.",
        "extern" => "Declare a binding to an external symbol.",
        "as" => "Convert a value to another type.",
        "self" => "Refer to the current instance inside a method.",
        "struct" => "Declare a struct type.",
        "enum" => "Declare an enum type.",
        "trait" => "Declare a trait with an interface contract.",
        "impl" => "Implement a trait for a type.",
        "if" => "Branch on a boolean condition.",
        "else" => "Run when every `if` and `elif` condition is false.",
        "elif" => "Branch on an additional condition.",
        "while" => "Loop while a condition holds.",
        "for" => "Loop over a range or a collection.",
        "in" => "Introduce the iterable of a `for` loop.",
        "return" => "Return a value from a function.",
        "pass" => "An empty statement, used as a placeholder.",
        "break" => "Leave a loop.",
        "continue" => "Skip to the next loop iteration.",
        "match" => "Branch on the value of an enum or scrutinee.",
        "case" => "One branch of a `match` or a pattern check.",
        "defer" => "Run a cleanup block when the function exits, even on error paths.",
        "unsafe" => "Run code with relaxed safety checks.",
        _ => "NEX language keyword.",
    };
    description.to_string()
}

fn resolve_symbol(symbols: &[SymbolInfo], name: &str, method_context: bool) -> Option<SymbolInfo> {
    if method_context {
        return symbols
            .iter()
            .find(|symbol| symbol.kind == SymbolKind::Method && symbol.name == name)
            .cloned();
    }
    if let Some(found) = symbols
        .iter()
        .find(|symbol| symbol.name == name && symbol.kind != SymbolKind::Method)
    {
        return Some(found.clone());
    }
    builtin_symbol(name).or_else(|| keyword_symbol(name))
}

fn hover_markdown(symbol: &SymbolInfo) -> String {
    let title = match symbol.kind {
        SymbolKind::Keyword => "keyword",
        SymbolKind::Builtin => "builtin",
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Enum => "enum",
        SymbolKind::Trait => "trait",
        SymbolKind::Const => "constant",
        SymbolKind::Global => "variable",
        SymbolKind::Alias => "type alias",
    };
    format!(
        "**`{}`** ({})\n\n```nex\n{}\n```\n\n{}",
        symbol.name, title, symbol.detail, symbol.documentation
    )
}

fn completion_item_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Keyword => 14,
        SymbolKind::Builtin => 3,
        SymbolKind::Function => 3,
        SymbolKind::Method => 2,
        SymbolKind::Struct => 22,
        SymbolKind::Enum => 13,
        SymbolKind::Trait => 8,
        SymbolKind::Const => 21,
        SymbolKind::Global => 6,
        SymbolKind::Alias => 25,
    }
}

fn document_symbol_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Keyword | SymbolKind::Builtin => 13,
        SymbolKind::Function => 12,
        SymbolKind::Method => 6,
        SymbolKind::Struct => 23,
        SymbolKind::Enum => 10,
        SymbolKind::Trait => 11,
        SymbolKind::Const => 14,
        SymbolKind::Global => 13,
        SymbolKind::Alias => 26,
    }
}

fn severity_number(severity: Severity) -> u32 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    }
}

fn diagnostic_to_lsp(diagnostic: &Diagnostic, source: &SourceFile) -> Value {
    let (start_line, start_col) = diagnostic
        .span
        .map(|span| source.line_col(span.start))
        .unwrap_or((1, 1));
    let (end_line, end_col) = diagnostic
        .span
        .map(|span| source.line_col(span.end))
        .unwrap_or((start_line, start_col));
    json!({
        "range": {
            "start": { "line": start_line.saturating_sub(1), "character": start_col.saturating_sub(1) },
            "end": { "line": end_line.saturating_sub(1), "character": end_col.saturating_sub(1) },
        },
        "severity": severity_number(diagnostic.severity),
        "code": diagnostic.code,
        "source": "nex",
        "message": diagnostic.message,
    })
}

fn diagnostics_for(uri: &str, text: &str) -> (Vec<Diagnostic>, SourceFile) {
    let source_name = uri.trim_start_matches("file://").to_string();
    let options = nex_driver::CompileOptions {
        emit: nex_driver::Emit::Check,
        profile: nex_driver::Profile::Debug,
        output: None,
        build_dir: None,
        target: None,
    };
    match nex_driver::compile_source(source_name.clone(), text.to_string(), options) {
        Ok(result) => (result.diagnostics, result.source),
        Err(error) => (
            vec![nex_diagnostics::Diagnostic::error_without_span(
                "NEX001",
                format!("cannot analyze NEX source: {error}"),
            )],
            SourceFile::new(source_name, text.to_string()),
        ),
    }
}

fn handle_hover(document: &Document, params: &Value) -> Result<Value, Value> {
    let position = position_of(params)
        .ok_or_else(|| json!({ "code": -32602, "message": "missing hover position" }))?;
    let offset = position_to_offset(&document.text, position.0, position.1);
    let Some((start, _end, word)) = identifier_at(&document.text, offset) else {
        return Ok(Value::Null);
    };
    if KEYWORDS.contains(&word.as_str()) {
        return Ok(json!({
            "contents": {
                "kind": "markdown",
                "value": format!("**keyword `{word}`**\n\n{}", keyword_documentation(&word)),
            },
        }));
    }
    let lexed = Lexer::new(&document.text).lex();
    let parsed = parse(&lexed.tokens);
    let program = parsed.program;
    let analysis = analyze(&program);
    let symbols = collect_symbols(&program, &analysis);
    let method_context = is_method_access(&document.text, start);
    let resolved = resolve_symbol(&symbols, &word, method_context);
    Ok(match resolved {
        Some(symbol) => json!({
            "contents": {
                "kind": "markdown",
                "value": hover_markdown(&symbol),
            },
            "range": {
                "start": position_json(&document.text, start),
                "end": position_json(&document.text, start + word.len()),
            },
        }),
        None => Value::Null,
    })
}

fn position_json(text: &str, offset: usize) -> Value {
    let (line, character) = offset_to_position(text, offset);
    json!({ "line": line, "character": character })
}

fn handle_definition(document: &Document, params: &Value) -> Result<Value, Value> {
    let position = position_of(params)
        .ok_or_else(|| json!({ "code": -32602, "message": "missing definition position" }))?;
    let offset = position_to_offset(&document.text, position.0, position.1);
    let Some((start, _end, word)) = identifier_at(&document.text, offset) else {
        return Ok(json!([]));
    };
    let lexed = Lexer::new(&document.text).lex();
    let parsed = parse(&lexed.tokens);
    let program = parsed.program;
    let analysis = analyze(&program);
    let symbols = collect_symbols(&program, &analysis);
    let method_context = is_method_access(&document.text, start);
    let resolved = resolve_symbol(&symbols, &word, method_context);
    let Some(symbol) = resolved else {
        return Ok(json!([]));
    };
    let Some(span) = symbol.span else {
        return Ok(json!([]));
    };
    Ok(json!([
        {
            "uri": document.uri,
            "range": {
                "start": position_json(&document.text, span.start),
                "end": position_json(&document.text, span.end),
            },
        }
    ]))
}

fn handle_completion(document: &Document, params: &Value) -> Result<Value, Value> {
    let position = position_of(params)
        .ok_or_else(|| json!({ "code": -32602, "message": "missing completion position" }))?;
    let offset = position_to_offset(&document.text, position.0, position.1);
    let prefix = current_word_prefix(&document.text, offset).to_lowercase();
    let lexed = Lexer::new(&document.text).lex();
    let parsed = parse(&lexed.tokens);
    let program = parsed.program;
    let analysis = analyze(&program);
    let symbols = collect_symbols(&program, &analysis);
    let mut items: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for symbol in &symbols {
        if !prefix.is_empty() && !symbol.name.to_lowercase().starts_with(&prefix) {
            continue;
        }
        if !seen.insert((symbol.name.clone(), symbol.kind as u32)) {
            continue;
        }
        items.push(json!({
            "label": symbol.name,
            "kind": completion_item_kind(symbol.kind),
            "detail": symbol.detail,
            "documentation": {
                "kind": "markdown",
                "value": symbol.documentation,
            },
        }));
    }
    for keyword in KEYWORDS {
        if !prefix.is_empty() && !keyword.starts_with(&prefix) {
            continue;
        }
        if seen.insert((keyword.to_string(), u32::MAX)) {
            items.push(json!({
                "label": keyword,
                "kind": 14,
                "detail": format!("keyword `{keyword}`"),
                "documentation": {
                    "kind": "markdown",
                    "value": keyword_documentation(keyword),
                },
            }));
        }
    }
    for (name, detail, documentation) in BUILTINS {
        if !prefix.is_empty() && !name.starts_with(&prefix) {
            continue;
        }
        if seen.insert((name.to_string(), u32::MAX - 1)) {
            items.push(json!({
                "label": name,
                "kind": 3,
                "detail": detail,
                "documentation": {
                    "kind": "markdown",
                    "value": documentation,
                },
            }));
        }
    }
    Ok(json!({ "isIncomplete": false, "items": items }))
}

fn document_symbol_json(name: &str, kind: u32, detail: &str, span: Span) -> Value {
    json!({
        "name": name,
        "detail": detail,
        "kind": kind,
        "range": {
            "start": { "line": span.start as u32, "character": 0 },
            "end": { "line": span.end as u32, "character": 0 },
        },
        "selectionRange": {
            "start": { "line": span.start as u32, "character": 0 },
            "end": { "line": span.end as u32, "character": 0 },
        },
    })
}

fn handle_document_symbols(document: &Document, _params: &Value) -> Result<Value, Value> {
    let lexed = Lexer::new(&document.text).lex();
    let parsed = parse(&lexed.tokens);
    let program = parsed.program;
    let mut symbols = Vec::new();
    for item in &program.structs {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Struct),
            "struct",
            item.span,
        ));
    }
    for item in &program.enums {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Enum),
            "enum",
            item.span,
        ));
    }
    for item in &program.traits {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Trait),
            "trait",
            item.span,
        ));
    }
    for item in &program.consts {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Const),
            "const",
            item.span,
        ));
    }
    for item in &program.globals {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Global),
            "var",
            item.span,
        ));
    }
    for item in &program.type_aliases {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Alias),
            "type alias",
            item.span,
        ));
    }
    for item in &program.functions {
        symbols.push(document_symbol_json(
            &item.name,
            document_symbol_kind(SymbolKind::Function),
            "fn",
            item.span,
        ));
    }
    for block in &program.impls {
        let mut children = Vec::new();
        for method in &block.methods {
            children.push(document_symbol_json(
                &method.name,
                document_symbol_kind(SymbolKind::Method),
                "method",
                method.span,
            ));
        }
        let mut detail = format!("impl {}", block.type_name);
        if !block.trait_name.is_empty() {
            detail = format!("impl {} for {}", block.trait_name, block.type_name);
        }
        symbols.push(json!({
            "name": detail,
            "kind": document_symbol_kind(SymbolKind::Struct),
            "range": {
                "start": { "line": block.span.start as u32, "character": 0 },
                "end": { "line": block.span.end as u32, "character": 0 },
            },
            "selectionRange": {
                "start": { "line": block.span.start as u32, "character": 0 },
                "end": { "line": block.span.end as u32, "character": 0 },
            },
            "children": children,
        }));
    }
    Ok(Value::Array(symbols))
}

fn handle_formatting(document: &Document, _params: &Value) -> Result<Value, Value> {
    let result = format_source(&document.text);
    if result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.is_error())
    {
        return Ok(json!([]));
    }
    if result.source == document.text {
        return Ok(json!([]));
    }
    let end = offset_to_position(&document.text, document.text.len());
    Ok(json!([
        {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": end.0, "character": end.1 },
            },
            "newText": result.source,
        }
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        Document {
            uri: "file:///tmp/main.nex".to_string(),
            version: 1,
            text: text.to_string(),
        }
    }

    #[test]
    fn position_offset_roundtrip() {
        let text = "fn main():\n    var x = 1\n";
        for offset in [0, 5, 12, 24, text.len()] {
            let (line, character) = offset_to_position(text, offset);
            let restored = position_to_offset(text, line, character);
            assert_eq!(offset, restored);
        }
    }

    #[test]
    fn identifier_at_inside_word() {
        let text = "fn hello(world: i64):\n";
        let offset = text.find("hello").unwrap() + 3;
        let (start, end, word) = identifier_at(text, offset).unwrap();
        assert_eq!(word, "hello");
        assert_eq!(&text[start..end], "hello");
    }

    #[test]
    fn identifier_at_requires_word_chars() {
        let text = "fn main():\n    print(1)\n";
        let offset = text.find(':').unwrap();
        assert!(identifier_at(text, offset).is_none());
        let offset = text.find("print").unwrap() + 5;
        let (_, _, word) = identifier_at(text, offset).unwrap();
        assert_eq!(word, "print");
    }

    #[test]
    fn current_word_prefix_at_end() {
        let text = "fn ma";
        assert_eq!(
            current_word_prefix(text, text.len()),
            "fn ma".chars().skip(3).collect::<String>()
        );
        let text = "fn main(ot";
        assert_eq!(current_word_prefix(text, text.len()), "ot");
    }

    #[test]
    fn hover_builtin_returns_signature() {
        let doc = document("fn main():\n    print(\"hi\")\n");
        let params = json!({
            "textDocument": { "uri": doc.uri },
            "position": { "line": 1, "character": 5 },
        });
        let response = handle_hover(&doc, &params).unwrap();
        assert!(!response.is_null());
        assert!(response.to_string().contains("print"));
    }

    #[test]
    fn diagnostics_for_valid_source_has_no_errors() {
        let (diagnostics, _source) =
            diagnostics_for("file:///tmp/valid.nex", "fn main():\n    print(1)\n");
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic.is_error()));
    }
}
