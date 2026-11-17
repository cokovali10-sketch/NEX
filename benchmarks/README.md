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
# Rust baseline (needs the Rust toolchain)
cargo build --release --manifest-path benchmarks/rust_server/Cargo.toml

# NEX server
nex build examples/http_server --release

# Load generator (its own crate, not part of the NEX workspace)
cargo build --release --manifest-path benchmarks/loadgen/Cargo.toml

$nex  = Start-Process .\examples\http_server\build\release\main.exe -PassThru
$rust = Start-Process .\target\release\nex-bench-axum.exe -PassThru
.\loadgen\target\release\nex-bench-loadgen.exe http://127.0.0.1:8080/user/1 10 50
.\loadgen\target\release\nex-bench-loadgen.exe http://127.0.0.1:8081/user/1 10 50
Stop-Process $nex.Id, $rust.Id
```

## Results on this machine (Windows 11, debug load generator, 50 connections, 5 s)

| Server | req/s | p50 | p95 | p99 | failed |
|---|---|---|---|---|---|
| NEX (debug) | 1 336 | 9.62 ms | 18.78 ms | 1023 ms | 3 |
| NEX (release) | 1 255 | 11.40 ms | 25.69 ms | 1025 ms | 2 |
| axum 0.7 | **47 377** | **0.99 ms** | **1.75 ms** | **2.29 ms** | 0 |

**axum is ~35x faster here.** The interesting part is why, because it is not the
codegen: NEX and Rust both go through LLVM, so the gap is architectural.

What NEX pays for, in order of impact:

1. **Thread per connection.** 50 concurrent clients means 50 OS threads and
   constant context switches. tokio multiplexes onto a fixed worker pool.
2. **No keep-alive.** Every request opens a new TCP connection; tokio reuses
   them. The p99 spike and the failed requests are the accept backlog (16)
   overflowing.
4. **One `tcp_read` per request**, then the connection closes.
5. **Per-request allocation**: the router splits the path into strings with
   `malloc`, and `http_response` builds the response with `snprintf`.

What this does *not* show: CPU-bound work. For hashing, parsing, or template
rendering the gap should be far smaller, because both hit the same backend. That
is the measurement worth doing next.

## What would change the numbers

- a thread pool with a bounded queue instead of thread-per-connection;
- keep-alive and a read loop until the body is complete;
- a router that matches without splitting into owned strings;
- `nex build --release` wired to `-O2` (today `-O0` in debug, and release only
   adds `-O3` for Rust, not for NEX).

Until then, this table is the honest state: **NEX is usable for a small server,
not yet competitive with a tuned Rust stack.**