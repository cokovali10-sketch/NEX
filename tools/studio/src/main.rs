//! NEX Studio — a tiny local web editor that compiles and runs NEX code.
//!
//! It is intentionally dependency-free (std only): an HTTP server on
//! 127.0.0.1 that serves a single-page editor and a `/run` endpoint which
//! writes the code to a temp file, invokes the `nexc` compiler to build an
//! executable, runs it, and returns stdout/stderr/exit code as JSON.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;

const INDEX_HTML: &str = include_str!("../static/index.html");

fn main() {
    let addr = "127.0.0.1:7788";
    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind {addr}: {error}");
            std::process::exit(1);
        }
    };

    let nexc = locate_nexc();
    match &nexc {
        Some(path) => eprintln!("using compiler: {}", path.display()),
        None => eprintln!(
            "warning: could not find `nexc`; build it first with `cargo build -p nex-driver`"
        ),
    }

    println!("NEX Studio running at http://{addr}");
    println!("Press Ctrl+C to stop.");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let nexc = nexc.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, nexc) {
                        eprintln!("connection error: {error}");
                    }
                });
            }
            Err(error) => eprintln!("accept error: {error}"),
        }
    }
}

/// Finds the `nexc` compiler: `$NEXC` if set, otherwise the workspace target
/// directory relative to this crate.
fn locate_nexc() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("NEXC") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe_name = if cfg!(windows) { "nexc.exe" } else { "nexc" };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("../../target/debug").join(exe_name),
        manifest.join("../../target/release").join(exe_name),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn handle_connection(mut stream: TcpStream, nexc: Option<PathBuf>) -> std::io::Result<()> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];

    // Read until we have the complete headers.
    let header_end = loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(end) = find_header_end(&buffer) {
            break end;
        }
        if buffer.len() > 4 * 1024 * 1024 {
            return Ok(());
        }
    };

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);

    let body_start = header_end + 4;
    // Read the remaining body bytes (blocking until we have them all).
    while buffer.len() < body_start + content_length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body_end = (body_start + content_length).min(buffer.len());
    let body = buffer.get(body_start..body_end).unwrap_or(&[]).to_vec();

    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                INDEX_HTML.len(),
                INDEX_HTML
            );
            stream.write_all(response.as_bytes())?;
        }
        ("POST", "/run") => {
            let code = String::from_utf8_lossy(&body).to_string();
            let result = run_code(&code, nexc.as_deref());
            let payload = json_escape(&result);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            );
            stream.write_all(response.as_bytes())?;
        }
        _ => {
            let response =
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(response.as_bytes())?;
        }
    }
    Ok(())
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

struct RunResult {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn run_code(code: &str, nexc: Option<&std::path::Path>) -> RunResult {
    let Some(nexc) = nexc else {
        return RunResult {
            stdout: String::new(),
            stderr: "compiler `nexc` not found. Build it with: cargo build -p nex-driver\n"
                .to_string(),
            exit: 127,
        };
    };

    let dir = std::env::temp_dir().join("nex_studio_run");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let source = dir.join("main.nex");
    if let Err(error) = std::fs::write(&source, code) {
        return RunResult {
            stdout: String::new(),
            stderr: format!("failed to write source: {error}\n"),
            exit: 1,
        };
    }

    let compile = Command::new(nexc)
        .arg("main.nex")
        .arg("--emit")
        .arg("exe")
        .current_dir(&dir)
        .output();

    let compile = match compile {
        Ok(output) => output,
        Err(error) => {
            return RunResult {
                stdout: String::new(),
                stderr: format!("failed to run compiler: {error}\n"),
                exit: 1,
            };
        }
    };

    let compile_stdout = String::from_utf8_lossy(&compile.stdout).to_string();
    let compile_stderr = String::from_utf8_lossy(&compile.stderr).to_string();

    let exe = dir
        .join("build")
        .join("debug")
        .join(if cfg!(windows) { "main.exe" } else { "main" });

    if !compile.status.success() || !exe.is_file() {
        return RunResult {
            stdout: compile_stdout,
            stderr: compile_stderr,
            exit: compile.status.code().unwrap_or(1),
        };
    }

    match Command::new(&exe).current_dir(&dir).output() {
        Ok(run) => RunResult {
            stdout: format!("{compile_stdout}{}", String::from_utf8_lossy(&run.stdout)),
            stderr: format!("{compile_stderr}{}", String::from_utf8_lossy(&run.stderr)),
            exit: run.status.code().unwrap_or(0),
        },
        Err(error) => RunResult {
            stdout: compile_stdout,
            stderr: format!("{compile_stderr}\nfailed to run executable: {error}\n"),
            exit: 1,
        },
    }
}

/// Builds a minimal JSON object string from a `RunResult` (no dependencies).
fn json_escape(result: &RunResult) -> String {
    format!(
        "{{\"stdout\":\"{}\",\"stderr\":\"{}\",\"exit\":{}}}",
        escape(&result.stdout),
        escape(&result.stderr),
        result.exit
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
