//! End-to-end smoke test for `nex lsp`: spawns the server binary, speaks
//! JSON-RPC over stdio and checks the handshake, diagnostics, hover,
//! definition, completion, document symbols, formatting and shutdown.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const SOURCE: &str = "\
fn helper(n: i64) -> i64:
    return n * 2

fn main():
    print(\"hello\")
    let answer = helper(40)
    print(answer)
";

struct LspClient {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl LspClient {
    fn spawn() -> Self {
        let exe = env!("CARGO_BIN_EXE_nex");
        let mut child = Command::new(exe)
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn `nex lsp`");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
        }
    }

    fn send(&mut self, message: serde_json::Value) {
        let body = message.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len())
            .expect("failed to write message");
        self.stdin.flush().expect("failed to flush message");
    }

    fn read(&mut self) -> serde_json::Value {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            self.reader
                .read_line(&mut line)
                .expect("failed to read header line");
            if line.trim().is_empty() {
                break;
            }
            if let Some(value) = line.trim().strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse::<usize>().expect("content length"));
            }
        }
        let length = content_length.expect("missing Content-Length header");
        let mut buffer = vec![0u8; length];
        self.reader
            .read_exact(&mut buffer)
            .expect("failed to read body");
        serde_json::from_slice(&buffer).expect("message must be JSON")
    }
}

fn position_json(source: &str, target: &str) -> (u32, u32) {
    let index = source
        .find(target)
        .unwrap_or_else(|| panic!("`{target}` not found"));
    let mut line = 0u32;
    let mut character = 0u32;
    for (byte, ch) in source.char_indices() {
        if byte == index {
            break;
        }
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    (line, character)
}

#[test]
fn nex_lsp_answers_a_full_session() {
    let mut client = LspClient::spawn();

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": "file:///tmp",
            "capabilities": {},
        },
    }));
    let response = client.read();
    assert_eq!(response["id"], 1);
    let capabilities = &response["result"]["capabilities"];
    assert_eq!(capabilities["textDocumentSync"], 1);
    assert_eq!(capabilities["hoverProvider"], true);
    assert_eq!(capabilities["definitionProvider"], true);
    assert_eq!(capabilities["completionProvider"]["resolveProvider"], false);
    assert_eq!(capabilities["documentFormattingProvider"], true);
    assert_eq!(capabilities["documentSymbolProvider"], true);
    assert_eq!(response["result"]["serverInfo"]["name"], "nex-lsp");

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {},
    }));

    let uri = "file:///tmp/nex-lsp-tests.nex";
    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "nex",
                "version": 1,
                "text": SOURCE,
            },
        },
    }));
    let notification = client.read();
    assert_eq!(notification["method"], "textDocument/publishDiagnostics");
    assert_eq!(notification["params"]["diagnostics"], serde_json::json!([]));

    let (hover_line, hover_character) = position_json(SOURCE, "helper(40)");
    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": hover_line, "character": hover_character },
        },
    }));
    let hover = client.read();
    assert_eq!(hover["id"], 2);
    let contents = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover contents must be markdown");
    assert!(contents.contains("helper"), "hover: {contents}");

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": hover_line, "character": hover_character },
        },
    }));
    let definition = client.read();
    assert_eq!(definition["id"], 3);
    let locations = definition["result"]
        .as_array()
        .expect("definition must return an array");
    assert_eq!(locations.len(), 1, "definition: {locations:?}");
    assert_eq!(locations[0]["uri"], uri);
    let definition_line = locations[0]["range"]["start"]["line"]
        .as_u64()
        .expect("definition line");
    assert_eq!(definition_line, 0, "definition must point at `fn helper`");

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": uri },
            "position": { "line": 5, "character": 0 },
        },
    }));
    let completion = client.read();
    assert_eq!(completion["id"], 4);
    let labels: std::collections::HashSet<&str> = completion["result"]["items"]
        .as_array()
        .expect("completion items")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(labels.contains("helper"), "labels: {labels:?}");
    assert!(labels.contains("print"), "labels: {labels:?}");

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/documentSymbol",
        "params": { "textDocument": { "uri": uri } },
    }));
    let symbols = client.read();
    assert_eq!(symbols["id"], 5);
    let names: std::collections::HashSet<&str> = symbols["result"]
        .as_array()
        .expect("document symbols")
        .iter()
        .filter_map(|symbol| symbol["name"].as_str())
        .collect();
    assert!(names.contains("helper"), "symbols: {names:?}");
    assert!(names.contains("main"), "symbols: {names:?}");

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        },
    }));
    let formatting = client.read();
    assert_eq!(formatting["id"], 6);
    assert!(
        formatting["result"].is_array(),
        "formatting must return an edit array"
    );

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "shutdown",
        "params": Value::Null,
    }));
    let shutdown = client.read();
    assert_eq!(shutdown["id"], 7);
    assert!(shutdown["result"].is_null());

    client.send(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "exit",
        "params": Value::Null,
    }));
    let status = client.child.wait().expect("server must exit");
    assert!(status.success());
}
