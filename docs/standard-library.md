# Standard Library

NEX ships a small runtime-backed standard library. The functions below are
implemented in `runtime/core/nex_rt.c`, declared in the LLVM backend, and type
checked by the semantic analyzer. The integration suite exercises them end to
end; a few low-level helpers are covered indirectly rather than by a dedicated
test.

Implemented modules: strings and formatting, files, blocking TCP/HTTP helpers,
JSON, string-keyed maps, threads/mutexes, a thread pool, a router, a string
builder, time and math. Still missing: general collections, cryptography,
process control, structured logging, and async I/O. See `ROADMAP.md`.

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
| `str_index_of_from` | `(value: string, needle: string, start: i64) -> i64` | Index of the first match at or after `start`, or `-1`. Use it with `str_substring` to split. |
| `char_at` | `(value: string, index: i64) -> string` | One-character string, or empty when the index is out of range. |
| `char_code` | `(value: string) -> i64` | Code of the first character, `-1` for an empty string. |
| `to_string` | `(value: i64 or f64 or bool) -> string` | Decimal representation, `%g` for floats, `true`/`false`. |
| `parse_i64` | `(value: string) -> i64` | Decimal parse; returns `0` when the text is not a number. |
| `str_split_count` | `(value: string, separator: string) -> i64` | Number of fields a split would produce. |
| `str_split_at` | `(value: string, separator: string, index: i64) -> string` | The nth field, or an empty string. |
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
primitives must use `thread_spawn` or `pool_submit` for concurrency (see
Threads). `tcp_read` returns whatever a single `recv` produced, so an
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

### Time

| Function | Signature | Notes |
|---|---|---|
| `unix_time` | `() -> i64` | Seconds since the Unix epoch (wall clock). |
| `now_ms` | `() -> i64` | Milliseconds from a monotonic clock: use it to measure durations. |
| `sleep_ms` | `(milliseconds: i64) -> void` | Blocks the calling thread. |
| `http_date` | `() -> string` | Current time as an RFC 7231 IMF-fixdate, e.g. `Sun, 15 Nov 2026 14:45:33 GMT`. |

`now_ms` is monotonic, so it never jumps when the system clock is adjusted;
`unix_time` is the wall clock and is the one to use for `Date` headers and
timestamps you persist.

### JSON

A read-only JSON DOM. `json_parse` returns a handle (`i64`), or `0` when the text
is not valid JSON. Handles are never freed: NEX has no ownership model yet.

| Function | Signature | Notes |
|---|---|---|
| `json_parse` | `(text: string) -> i64` | Handle, or `0` on invalid input. |
| `json_type` | `(handle: i64) -> string` | `null`, `bool`, `number`, `string`, `array`, `object`; empty for handle `0`. |
| `json_len` | `(handle: i64) -> i64` | Array elements or object members. |
| `json_at` | `(handle: i64, index: i64) -> i64` | Array item; `0` when out of range or not an array. |
| `json_get` | `(handle: i64, key: string) -> i64` | Object member; `0` when absent. |
| `json_str` | `(handle: i64) -> string` | String value, or empty. |
| `json_num` | `(handle: i64) -> f64` | Number value. |
| `json_bool` | `(handle: i64) -> bool` | Boolean value. |
| `json_is_null` | `(handle: i64) -> bool` | True for JSON `null` and for handle `0`. |
| `json_stringify` | `(handle: i64) -> string` | Compact JSON text, with escaping. |

```nex
fn main():
    var doc = json_parse("{\"name\": \"NEX\", \"tags\": [\"a\", \"b\"]}")
    println(json_type(doc))                      // object
    println(json_str(json_get(doc, "name")))     // NEX
    println(json_len(json_get(doc, "tags")))     // 2
```

`json_stringify` serialises a handle back to compact JSON; the DOM is still
read-only, so round-tripping a parsed document works but there is no way to
construct one from NEX values yet.

### Splitting

`str_split_count` and `str_split_at` locate fields but do not build a collection,
because a split needs a growable array of strings. Compose them in NEX:

```nex
fn split(text: string, separator: string) -> [string]:
    var out: [string] = []
    var i = 0
    var total = str_split_count(text, separator)
    while i < total:
        push(out, str_split_at(text, separator, i))
        i = i + 1
    return out
```

### Maps

A string-keyed, string-valued hash map. `map_new` returns a handle (`i64`);
entries are never freed until the process exits.

| Function | Signature | Notes |
|---|---|---|
| `map_new` | `() -> i64` | New empty map. |
| `map_set` | `(handle: i64, key: string, value: string) -> bool` | Insert or overwrite. |
| `map_get` | `(handle: i64, key: string) -> string` | Empty string when absent. |
| `map_has` | `(handle: i64, key: string) -> bool` | Membership test. |
| `map_remove` | `(handle: i64, key: string) -> bool` | True when a key was removed. |
| `map_len` | `(handle: i64) -> i64` | Number of entries. |
| `map_key_at` | `(handle: i64, index: i64) -> string` | nth key, or empty. Iteration order is unspecified. |

```nex
fn main():
    var headers = map_new()
    map_set(headers, "content-type", "application/json")
    println(map_get(headers, "content-type"))
```

### Threads

Threads are OS threads. The entry point is named as a string. It either takes no arguments or a single
`i64`; return values are not supported yet.

| Function | Signature | Notes |
|---|---|---|
| `thread_spawn` | `("name") -> i64` | Handle, or `0` when the thread could not start. |
| `thread_spawn` | `("name", arg: i64) -> i64` | Same, passing one `i64` argument to the worker. |
| `thread_join` | `(handle: i64) -> bool` | Waits for completion. |
| `thread_running` | `(handle: i64) -> bool` | False once the thread has finished. |

```nex
fn worker():
    print("worked")

fn main():
    var handle = thread_spawn("worker")
    thread_join(handle)
```

Mutexes are listed below; channels and thread-safe collections still do not exist.

| Function | Signature | Notes |
|---|---|---|
| `mutex_new` | `() -> i64` | Handle to a new mutex. |
| `mutex_lock` | `(handle: i64) -> void` | Blocks until the mutex is free. |
| `mutex_unlock` | `(handle: i64) -> void` | Releases the mutex. |

```nex
var lock = 0
var total = 0

fn bump():
    mutex_lock(lock)
    total = total + 1
        mutex_unlock(lock)
```

### Router

Matches a method and a path pattern like `/user/:id`. Params are captured and
read back by name.

| Function | Signature | Notes |
|---|---|---|
| `router_new` | `() -> i64` | New empty router. |
| `router_add` | `(handle: i64, method: string, pattern: string) -> i64` | Route id (1-based). |
| `router_match` | `(handle: i64, method: string, path: string) -> i64` | `0` = no route (404), `-1` = path exists under another method (405), otherwise the route id. |
| `router_param` | `(handle: i64, name: string) -> string` | Captured `:name`, or empty. |

```nex
var r = router_new()
router_add(r, "GET", "/user/:id")

fn main():
    var id = router_match(r, "GET", "/user/42")
    if id > 0:
        println(router_param(r, "id"))   // 42
```

### Thread pool

A fixed set of worker threads with a bounded queue. Use it instead of one thread
per connection.

| Function | Signature | Notes |
|---|---|---|
| `pool_new` | `(workers: i64, capacity: i64) -> i64` | Handle, or `0` on failure. |
| `pool_submit` | `(handle: i64, "name": string, arg: i64) -> bool` | `false` when the queue is full. |
| `pool_workers` | `(handle: i64) -> i64` | Number of worker threads. |

```nex
var pool = pool_new(8, 256)

fn serve(client: i64):
    ...

while true:
    var client = tcp_accept(server)
    pool_submit(pool, "serve", client)
```

### Splitting a string

Splitting is not a builtin yet: it needs a growable array of strings, and NEX can
already express that. Until a `str_split` primitive lands, use this helper:

```nex
fn split(text: string, separator: string) -> [string]:
    var out: [string] = []
    var i = 0
    var total = str_split_count(text, separator)
    while i < total:
        push(out, str_split_at(text, separator, i))
        i = i + 1
    return out

fn main():
    var query = split("id=7&page=2", "&")
    println(len(query))    // 2
    println(query[1])      // page=2
```

`str_split_count` and `str_split_at` are backed by `strstr`, so this is O(n) in
the number of fields for the count and O(n) per field lookup.

### Writing JSON

`json_parse` reads a document. To produce one, escape strings and assemble with a
builder:

| Function | Signature | Notes |
|---|---|---|
| `json_escape` | `(value: string) -> string` | Escapes `"`, `\`, CR, LF, TAB and control chars. |
| `json_quote` | `(value: string) -> string` | `json_escape` plus surrounding quotes. |

```nex
fn record(name: string, age: i64) -> string:
    var b = builder_new()
    builder_append(b, "{")
    builder_append(b, concat("\"name\": ", json_quote(name)))
    builder_append(b, concat(", \"age\": ", to_string(age)))
    builder_append(b, "}")
    return builder_string(b)
```

Numbers use `to_string` (integers) or the default `print` formatting for floats;
`json_stringify` serialises a parsed document back to compact JSON.

### String builder

`concat` allocates a new buffer on every call, so appending in a loop is
quadratic: 50 000 appends copy ~1.25 GB. Use a builder when building a string
piece by piece — appending is amortised O(1).

| Function | Signature | Notes |
|---|---|---|
| `builder_new` | `() -> i64` | New empty builder. |
| `builder_append` | `(handle: i64, text: string) -> bool` | Appends; `false` when out of memory. |
| `builder_len` | `(handle: i64) -> i64` | Length so far. |
| `builder_string` | `(handle: i64) -> string` | Materialises a copy. |
| `builder_clear` | `(handle: i64) -> void` | Empties without freeing the buffer. |

Measured on `examples/bench_strings` (50 000 appends of one byte):
`concat` 761 ms, `builder` 1 ms, identical results.

### Numeric

| Function | Signature | Notes |
|---|---|---|
| `abs` | `(value: i64 or f64) -> i64 or f64` | Absolute value, same type in and out. |
| `min` / `max` | `(a, b) -> i64 or f64` | Same-type operands only. |
| `sqrt` | `(value: f64) -> f64` | `libm` |
| `floor` / `ceil` | `(value: f64) -> f64` | `libm` rounding toward -inf / +inf. |
| `round` | `(value: f64) -> f64` | Round half away from zero. |
| `trunc` | `(value: f64) -> f64` | Drop the fractional part. |
| `pow` | `(base: f64, exponent: f64) -> f64` | `libm` |

### Diagnostics and memory

| Function | Signature | Notes |
|---|---|---|
| `assert` | `(condition: bool) -> void` | Panics when false. |
| `panic` | `(message: string) -> void` | Prints to stderr and exits with a non-zero status. |
| `alloc` | `(size: i64) -> i64` | Raw `malloc` as an address-sized integer. |
| `free` | `(address: i64) -> void` | Raw `free`. |
| `peek` | `(address: i64) -> i64` | Raw `i64` load for NEX Systems programs. |
| `poke` | `(address: i64, value: i64) -> void` | Raw `i64` store for NEX Systems programs. |

## Example

```nex
fn main():
    var text = "  Hello NEX  "
    print(str_upper(str_trim(text)))   // HELLO NEX
    println(str_index_of(text, "NEX")) // 8
```