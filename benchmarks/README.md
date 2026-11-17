# Benchmarks

One endpoint, two servers, one load generator. The goal is a number you can
reproduce, not a claim.

| Server | Stack |
|---|---|
| NEX | `examples/http_server` — NEX source, compiled by NEX |
| Rust | `rust_server` — axum 0.7 + tokio, same route and JSON |

Both answer `GET /user/:id` with `{"id": "<id>"}` on `127.0.0.1` (NEX on 8080,
Rust on 8081).

## Running it

```powershell
cargo build --release --manifest-path benchmarks/rust_server/Cargo.toml
cargo build --release --manifest-path benchmarks/loadgen/Cargo.toml
nex build examples/http_server --release

$nex  = Start-Process .\examples\http_server\build\release\main.exe -PassThru
$rust = Start-Process .\benchmarks\rust_server\target\release\nex-bench-axum.exe -PassThru
.\benchmarks\loadgen\target\release\nex-bench-loadgen.exe http://127.0.0.1:8080/user/1 10 50
.\benchmarks\loadgen\target\release\nex-bench-loadgen.exe http://127.0.0.1:8081/user/1 10 50
Stop-Process $nex.Id, $rust.Id
```

## Results on this machine (Windows 11, 50 connections, 5 s)

| Server | req/s | p50 | p95 | p99 | failed |
|---|---|---|---|---|---|
| NEX, thread per connection | 1 255 | 11.4 ms | 25.7 ms | 1025 ms | 2 |
| **NEX, thread pool (8 workers)** | **2 531** | 18.9 ms | 30.0 ms | 41.0 ms | **0** |
| axum 0.7 | **47 377** | 0.99 ms | 1.75 ms | 2.29 ms | 0 |

Both NEX builds use `-O2` (verified: `Profile::clang_optimization()` maps
`Release => "-O2"`), so this is not a debug-vs-release comparison.

The pool doubled throughput and, more importantly, removed the 1-second tail:
thread-per-connection was dropping connections into the accept backlog, which
showed up as p99 ≈ 1 s and failed requests.

**axum is still ~19x faster.** The remaining gap is architectural, not codegen:

1. **No keep-alive**: every request opens a new TCP connection; tokio reuses them.
3. **Per-request allocation**: the router splits the path into owned strings, and
   `http_response` builds the response with `snprintf`.
4. **One `tcp_read` per request**, then the connection closes.
5. **8 workers on a 50-connection load** — the queue is not the bottleneck yet,
   but workers are saturated at this rate.

What this does *not* measure: CPU-bound work. For hashing, parsing, or template
rendering the gap should be far smaller, because both go through the same LLVM.
That is the next measurement.

## Honest reading

**NEX can serve a real page, not yet compete with a tuned Rust stack.** The
optimisation list above is ordered by expected gain: keep-alive first, then a
router that matches without allocating, then more workers.