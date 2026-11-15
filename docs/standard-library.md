# Standard Library

NEX ships a small runtime-backed standard library. Every function below is
implemented in `runtime/core/nex_rt.c`, declared in the LLVM backend, type
checked by the semantic analyzer, and covered by an end-to-end test.

The remaining modules (collections, filesystem, networking, concurrency,
cryptography, serialization, time, logging, process) are not implemented yet.
See `ROADMAP.md` for the milestone they belong to.

## Builtins

These are global functions, not methods, so no imports are required.

### Printing

| Function | Signature | Notes |
|---|---|---|
| `print` | `(value) -> void` | Accepts `string`, `i64`, `u64`, `f64`, `bool`, `char`, structs, arrays, tuples. |
| `println` | `(value) -> void` | Same values as `print`, followed by a newline. |

### Strings

| Function | Signature | Notes |
|---|---|---|
| `len` | `(value: string or array) -> i64` | Byte length of a string; element count of an array. |
| `concat` | `(left: string, right: string) -> string` | Returns a newly allocated string. |
| `str_eq` | `(left: string, right: string) -> bool` | Content comparison. |
| `str_cmp` | `(left: string, right: string) -> i64` | Lexicographic comparison: `-1`, `0`, or `1`. |
| `str_contains` | `(haystack: string, needle: string) -> bool` | Substring search. |
| `str_index_of` | `(haystack: string, needle: string) -> i64` | Byte offset of the first match, or `-1`. |
| `str_substring` | `(value: string, start: i64, end: i64) -> string` | Bytes in `[start, end)`; out-of-range bounds are clamped, not errors. |
| `str_upper` | `(value: string) -> string` | ASCII-aware uppercase copy. |
| `str_lower` | `(value: string) -> string` | ASCII-aware lowercase copy. |
| `str_trim` | `(value: string) -> string` | Removes leading and trailing spaces, tabs, CR, and LF. |
| `str_starts_with` | `(value: string, prefix: string) -> bool` | Prefix test. |
| `str_ends_with` | `(value: string, suffix: string) -> bool` | Suffix test. |
| `str_repeat` | `(value: string, count: i64) -> string` | Repeats `count` times; `0` or less gives an empty string. |
| `str_replace` | `(value: string, from: string, to: string) -> string` | Replaces every occurrence; an empty `from` returns a copy. |
| `to_string` | `(value: i64 or f64 or bool) -> string` | Decimal representation, `%g` for floats, `true`/`false`. |
| `parse_i64` | `(value: string) -> i64` | Decimal parse; returns `0` when the text is not a number. |
| `str_reverse` | `(value: string) -> string` | Byte-wise reverse. |

String builtins that return a string allocate with `malloc`; NEX does not free
them yet because there is no ownership model (see `docs/memory-model.md`).

### Files

| Function | Signature | Notes |
|---|---|---|
| `read_file` | `(path: string) -> string` | Whole file as a string; empty string when it cannot be read. |
| `write_file` | `(path: string, content: string) -> bool` | Creates or truncates the file; `false` on failure. |
| `file_exists` | `(path: string) -> bool` | True when the path can be opened for reading. |
| `file_size` | `(path: string) -> i64` | Size in bytes, or `-1` when the file cannot be opened. |

There is no streaming API yet: `read_file` loads the whole file, so it is not
suitable for very large inputs. Paths are interpreted by the host operating
system, so use `/` on Windows too.

### Networking

Blocking TCP sockets. Handles are `i64` file descriptors; `-1` means failure.

| Function | Signature | Notes |
|---|---|---|
| `tcp_listen` | `(port: i64, backlog: i64) -> i64` | Returns a listening handle, or `-1`. |
| `tcp_accept` | `(server: i64) -> i64` | Blocks until a client connects. |
| `tcp_connect` | `(host: string, port: i64) -> i64` | Connects to a host; `-1` on failure. |
| `tcp_write` | `(fd: i64, data: string, length: i64) -> i64` | Bytes sent, or `-1`. |
| `tcp_read` | `(fd: i64, max_bytes: i64) -> string` | One read, capped at 256 KB; `""` on EOF or error. |
| `tcp_close` | `(fd: i64) -> bool` | Closes the handle. |

All calls block. There is no async runtime yet, so a server built from these
primitives serves one connection at a time and needs `threads` (not implemented)
for concurrency. `tcp_read` returns whatever a single `recv` produced, so an
HTTP server must loop until it sees the full request.

### HTTP

Request parsing and response building on top of the socket helpers. These are
string functions, so they work on a request head read with `tcp_read`.

| Function | Signature | Notes |
|---|---|---|
| `http_method` | `(request: string) -> string` | `GET`, `POST`, ... |
| `http_path` | `(request: string) -> string` | Request target, e.g. `/hello`. |
| `http_version` | `(request: string) -> string` | `HTTP/1.1`. |
| `http_header` | `(request: string, name: string) -> string` | Case-insensitive lookup; empty string when absent. |
| `http_body` | `(request: string) -> string` | Everything after the first empty line. |
| `http_response` | `(status: string, content_type: string, body: string) -> string` | Full `HTTP/1.1` response with `Content-Length` and `Connection: close`. |

A minimal server looks like this:

```nex
fn handle(request: string) -> string:
    var path = http_path(request)
    if path == "/":
        return http_response("200 OK", "text/plain; charset=utf-8", "hello")
    return http_response("404 Not Found", "text/plain; charset=utf-8", "not found")

fn main():
    var server = tcp_listen(8080, 8)
    var client = tcp_accept(server)
    var request = tcp_read(client, 8192)
    var response = handle(request)
    tcp_write(client, response, len(response))
    tcp_close(client)
    tcp_close(server)
```

This serves one request and exits: there is no `threads` support yet, so a real
server must loop and handle one connection at a time.

### Numeric

| Function | Signature | Notes |
|---|---|---|
| `abs` | `(value: i64 or f64) -> i64 or f64` | Absolute value, same type in and out. |
| `min` / `max` | `(a, b) -> i64 or f64` | Same-type operands only. |
| `sqrt` | `(value: f64) -> f64` | `libm` |
| `pow` | `(base: f64, exponent: f64) -> f64` | `libm` |

### Diagnostics and memory

| Function | Signature | Notes |
|---|---|---|
| `assert` | `(condition: bool) -> void` | Panics when false. |
| `panic` | `(message: string) -> void` | Prints to stderr and exits with a non-zero status. |
| `alloc` | `(size: i64) -> i64` | Raw `malloc` as an address-sized integer. |
| `free` | `(address: i64) -> void` | Raw `free`. |
| `peek_i64` / `poke_i64` | `(address[, value])` | Raw memory access for NEX Systems programs. |

## Example

```nex
fn main():
    var text = "  Hello NEX  "
    print(str_upper(str_trim(text)))   // HELLO NEX
    println(str_index_of(text, "NEX")) // 8
```