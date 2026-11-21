# NEX for AI Agents

This page is a single, self-contained reference for AI agents (assistants,
code-generation systems and human programmers alike) that need to read,
write, check and run NEX programs. It replaces guessing: everything below is
implemented by the NEX compiler in this repository, and every feature is
exercised by the test suite.

## What NEX is

NEX is a statically typed, Python-like, natively compiled programming
language. A NEX program is compiled to LLVM IR and linked by `clang` into a
native executable. There is no interpreter, no VM and no JIT. The toolchain
is a single binary, `nex`.

## The AI workflow

The loop that works for NEX is the same loop that works for any compiled
language:

1. Write the source with `Write`/`Edit`.
2. `nex check --json <path>`: type-check and get diagnostics as JSON. Exit
   code is `0` when the program is clean.
3. Fix the diagnostics, repeat until clean.
4. `nex fmt --write <file>`: canonicalize formatting.
5. `nex lint <file>`: catch suspicious code.
6. `nex run <path>`: build and run.

Do not invent built-in functions. The complete list is under
[Standard library](#standard-library). If the name is not in that list, it
does not exist. Prefer fixing the source over disabling checks.

## Project layout

```
my_project/
├── nex.toml
└── src/
    └── main.nex
```

```toml
[package]
name = "my_project"
version = "0.1.0"

[build]
entry = "src/main.nex"
```

`nex new my_project` creates this layout. `nex build`, `nex run`, `nex check`,
`nex test`, `nex fmt`, `nex lint`, `nex lsp`, `nex doctor` all work from the
project root or on a single file.

## Core language, in one page

### Comments

`#` starts a comment. The lexer keeps comments in a trivia table (used by
`nex fmt` and `nex lsp`) and otherwise ignores them; they are allowed
anywhere whitespace is allowed, including as the first lines of a block
(blank lines are also legal there). `nex fmt` preserves them.

```nex
# a comment
fn add(a: i64, b: i64) -> i64:
    # comment as the first line of a body
    return a + b
```

### Bindings

```nex
let x = 10                # immutable, type inferred
var y = 20                # mutable
y = y + x                 # assignment
x, y = y, x               # parallel assignment (swap)
var names: [string] = []  # annotated growable array
```

Top-level `var` is shared by every function and every thread (see Globals).
Top-level `let` is not allowed. `const` is a compile-time constant.

### Types

Scalars: `i32 i64 u32 u64 f64 f32 bool char string void`.
Composite:

| Shape | Meaning |
|---|---|
| `[T; N]` | fixed-size array, length in the type |
| `[T]` | growable array, starts empty |
| `(T, U)` | tuple |
| `struct` | user-defined composite |
| `enum` | user-defined sum type |
| `T<U>` | generic type argument, e.g. `Box<i64>` |

Declarations:

```nex
struct Point:
    x: i64
    y: i64

impl Point:                      # inherent methods (static dispatch)
    fn sum(self) -> i64:
        return self.x + self.y

enum Shape:
    Circle
    Square
    Dot

fn classify(shape: Shape) -> i64:
    match shape:
        case Circle:
            return 0
        case Square:
            return 1
        case Dot:
            return 2

struct Pair<T>:
    first: T
    second: T

fn pick<T>(a: T, b: T, choose_first: bool) -> T:
    if choose_first:
        return a
    return b

type Id = i64                     # type alias
const LIMIT: i64 = 100            # compile-time constant
```

Traits and implementations use **static dispatch** (like Rust's trait
impls, monomorphized at every call site):

```nex
trait Describe:
    fn label(self) -> string

struct Item:
    name: string

impl Describe for Item:
    fn label(self) -> string:
        return self.name

fn main():
    var item = Item("widget")
    println(item.label())
```

Generics are monomorphized: every distinct type argument produces a
specialized copy of the function or struct. Type arguments in annotations
are written `Box<i64>`; generic calls infer their arguments (`Box(42)`).

### Control flow

```nex
if x < 0:
    print("negative")
elif x == 0:
    print("zero")
else:
    print("positive")

while x > 0:
    x = x - 1

for i in 0..10:            # ranges: 0..9, end exclusive
    total = total + i

for name in names:         # collections: iterates by index
    println(name)

break
continue
pass                       # empty statement, required in empty blocks
```

`for x in array` is desugared by the parser into an index loop; it works for
`[T; N]`, `[T]`, `string` is **not** iterable (use `str_split*` / `char_at`).

### Error propagation and cleanup

`?` works on structs shaped like `Option` or `Result`:

```nex
fn find(xs: [i64], target: i64) -> Option<i64>:
    for x in xs:
        if x == target:
            return Option(true, x)
    return Option(false, 0)

fn describe(target: i64) -> Option<i64>:
    let xs: [i64] = [1, 2, 3]
    let found = find(xs, target)?   # early return on None
    return Option(true, found * 10)
```

The function must return the same `Option`/`Result` shape. NEX has
built-in generic `Option<T>` (fields `has: bool`, `value: T`) and
`Result<T, E>` (fields `ok: bool`, `value: T`, `error: E`) structs: no
declaration is required and `?` works on them out of the box. They are
also the reference shapes; user-declared structs with the same field names
get the same treatment via the `?` convention. Redeclaring `Option` or
`Result` in a project is an error (duplicate declaration).

`defer:` runs a cleanup block at function exit, in reverse order, but only
along paths that reach it:

```nex
fn copy_file(src: string, dst: string) -> bool:
    defer:
        print("cleanup always runs on reached paths")
    let text = read_file(src)
    if not write_file(dst, text):
        return false
    return true
```

### Globals

```nex
var counter = 0            # shared by all functions and threads
var lock = 0

fn bump():
    mutex_lock(lock)
    counter = counter + 1
    mutex_unlock(lock)

fn main():
    lock = mutex_new()
    var a = thread_spawn("bump")
    var b = thread_spawn("bump")
    thread_join(a)
    thread_join(b)
    println(counter)
```

Globals are inferred and shared, which is what makes mutex-guarded shared
state possible.

### Match

```nex
fn describe(code: i64) -> string:
    match code:
        case 0:
            return "zero"
        case 1..9:
            return "small"
        else:
            return "other"
```

Match arms: `case <pattern> [if <guard>]:` or `else:`. Patterns: integer
literals (including negative), inclusive ranges (`1..9`), booleans, chars,
strings, enum variants. Scalar `match` requires an `else` arm.

### Unsafe

```nex
unsafe:
    var address = alloc(64)
    poke(address, 42)
    println(peek(address))
    free(address)
```

`unsafe:` opts out of safety checks for raw memory work: `alloc`, `free`,
`peek`, `poke`.

### Comments and the formatter

`nex fmt` preserves `#` comments: a comment on the same line as a statement
stays on that line; a comment on its own line is placed before the following
statement at the same indent. Module-level `var` declarations are preserved
too. Blank lines and comment-only lines are legal as the first line of a
block; the formatter normalizes this.

## Standard library

These are global builtins. No imports are required. The full reference with
per-function notes lives in `docs/standard-library.md`; the most used are:

| Builtin | Signature | Notes |
|---|---|---|
| `print` / `println` | `(value) -> void` | structs/arrays/tuples print too; `println` adds newline |
| `len` | `(string or array) -> i64` | element count or byte length |
| `push` / `pop` | `(array, value) -> ...` | grow a growable array / remove last |
| `contains` / `index_of` / `reverse` | `(array, ...) -> ...` | value membership and reversal |
| `sort` / `join` | `(array[, string]) -> void/string` | ascending sort of `i64`/`f64`/`string`; string join |
| `sum_of` / `min_of` / `max_of` / `count` | `(array[, value]) -> ...` | numeric reductions and element count |
| `concat` / `to_string` | `(string, string) -> string` / `(value) -> string` | string building |
| `str_substring` / `str_index_of_from` | `(string, i64[, i64]) -> ...` | byte ranges |
| `str_contains` / `str_upper` / `str_lower` / `str_trim` | `(string) -> ...` | case and whitespace |
| `read_file` / `write_file` / `file_exists` / `file_size` | `(string[, string]) -> ...` | whole-file I/O |
| `tcp_listen` / `tcp_accept` / `tcp_connect` / `tcp_read` / `tcp_write` / `tcp_close` / `tcp_set_timeout` | `(...) -> i64` | blocking sockets, `-1` = failure |
| `http_method` / `http_path` / `http_header` / `http_body` / `http_response` / `parse_i64` | `(request) -> string` / `(status, type, body) -> string` | HTTP plumbing |
| `router_new` / `router_add` / `router_match` / `router_param` | `(...) -> i64/string` | `:param` path capture |
| `pool_new` / `pool_submit` / `pool_workers` | `(i64, i64) -> i64/bool` | fixed worker pool |
| `thread_spawn` / `thread_join` / `thread_running` / `mutex_new` / `mutex_lock` / `mutex_unlock` | `(...) -> i64/bool` | OS threads + mutexes |
| `map_new` / `map_set` / `map_get` / `map_has` / `map_remove` / `map_len` / `map_key_at` | `(string[, string]) -> ...` | string-keyed map |
| `json_parse` / `json_type` / `json_len` / `json_at` / `json_get` / `json_str` / `json_num` / `json_bool` / `json_stringify` | `(...) -> i64/...` | read-only JSON DOM |
| `json_escape` / `json_quote` | `(string) -> string` | emit JSON string values |
| `builder_new` / `builder_append` / `builder_string` / `builder_clear` | `(...) -> i64/string` | O(1) amortized string building |
| `unix_time` / `now_ms` / `sleep_ms` / `http_date` | `() -> i64/string` | wall and monotonic time |
| `env` | `(string) -> string` | environment variable; `""` when unset |
| `args_count` / `arg_at` | `() -> i64` / `(i64) -> string` | user CLI args (no `argv[0]`); `""` out of range |
| `exit` | `(i64) -> void` | flush stdout/stderr, terminate with the status code |
| `log_info` / `log_warn` / `log_error` | `(string) -> void` | one timestamped line per call on stderr |
| `abs` / `min` / `max` / `sqrt` / `pow` / `floor` / `ceil` / `round` / `trunc` | `(...) -> ...` | math |
| `parse_i64` | `(string) -> i64` | `0` on bad input |
| `assert(condition)` / `panic(message)` | `-> void` | abort paths |
| `str_split_count` / `str_split_at` | `(string, string) -> i64/string` | compose `split` yourself |

If it is not in that table, do not call it. Anything NEX cannot express is a
[known limitation](#limitations), not a hidden feature.

## Ambitious project: kv_store

`examples/kv_store` is a complete, working JSON HTTP key-value service. It is
verified end-to-end in this repository (see its `README.md` for the exact
`curl` commands). All of these pieces really compile and run:

- `tcp_listen`/`tcp_accept` accept loop with `tcp_set_timeout`;
- `router_match` routing with `:param` path capture;
- a `POST /kv/set` handler that parses a JSON body with `json_parse`;
- a `GET /kv/get/:key` handler with a mutex-guarded `map_*` store;
- `json_quote` + `concat` response assembly;
- a `pool_*` worker pool for connection handling.

The full source is `examples/kv_store/src/main.nex`. When building a bigger
project, start from it.

## Machine-readable diagnostics

`nex check --json <path>` prints:

```json
{
  "ok": false,
  "diagnostics": [
    {
      "severity": "error",
      "code": "NXS006",
      "message": "type mismatch: expected `i64`, found `string`",
      "file": "src/main.nex",
      "line": 5,
      "column": 12,
      "span": {"start": 78, "end": 82},
      "notes": ["help text when present"]
    }
  ]
}
```

Exit code `0` = no errors, `1` = diagnostics contain errors, `2` = toolchain
failure. Use `nex check --json` to drive automated fixes; use `nex check` for
human-readable output.

`nex lsp` provides the same compiler as a Language Server (hover,
go-to-definition, completion, document symbols, formatting, live diagnostics);
see `docs/language-server.md` for the LSP handshake and capabilities.

## Common error codes and what they mean for an agent

| Code | Meaning | Typical fix |
|---|---|---|
| `NXP002` | Expected an indented block after `:` | indent the body; keep the block non-empty with at least one statement or `pass` |
| `NXP005` | Expected a newline | one statement per line |
| `NXL005` | Inconsistent indentation | use consistent spaces |
| `NXS001` | Unknown item / unknown name | check spelling; it may be a module-qualified name |
| `NXS003` | Duplicate declaration | rename the duplicate |
| `NXS004` | Wrong number or type of arguments | match the signature exactly |
| `NXS005` | Bad destructuring arity | bind as many names as the value has fields |
| `NXS006` | Type mismatch | convert with `as`; check the annotated types |
| `NXS008` | Incompatible operand types | same-type arithmetic only (`i64` with `i64`, `u64` with `u64`, `f64` with `f64`) |
| `NXS010` | Missing return path | add `return` or change the return type |
| `NXS011` | `fn main` missing or wrong | entry point must be `fn main()` with no parameters, returning `void` or `i32` |
| `NXS016` | Assignment to undeclared variable | declare with `var`/`let` first |
| `NXS017` | Unreachable statement | remove dead code |
| `NXS027` | No such field | check field names |
| `NXS036` | Index is not an integer / cannot index | use `i64`; only arrays and strings are indexable |
| `NXS004` + `?` | `?` outside Option/Result | return `Option`/`Result` from the enclosing function |

## Limitations (do not work around them by disabling checks)

- No ownership/borrowing/lifetimes yet. Strings and arrays allocated by
  builtins (`concat`, `str_*`, JSON handles, map handles) are never freed;
  processes rely on OS cleanup at exit.
- Not everything is mutable through a const: `let` is immutable, `var` is
  mutable.
- No async/await. Servers are blocking TCP + threads/pools.
- Generics are monomorphized; there is no trait object or dynamic dispatch.
- JSON DOM is read-only.
- `for x in array` iterates by index; arrays share one buffer when assigned
  from another name, so mutating through any alias is visible through all of
  them (see `docs/memory-model.md`).
- Strings are indexable (`s[1]`) and sliceable (`s[1..3]`), but are not
  iterable with `for`.
- `nex fmt` preserves `#` comments and module-level `var` declarations; the
  output is syntactically valid NEX, but blank lines are normalized.
- Cross-platform claims: Windows x86-64 is the primary tested host; CI also
  tests Ubuntu. macOS is untested. Do not claim platform support you have not
  measured.
- Printing a struct/array/tuple is supported, but there is no derive or
  user-controlled formatting yet.

## Style for NEX code written by an agent

- `snake_case` for functions and variables; `PascalCase` for types.
- One statement per line; four-space indentation; no tabs.
- Prefer `println` for output.
- Annotate locals that need a type (`var x: i64 = 0`); inference handles the rest.
- Keep functions short; extract helper functions instead of nesting deeply.
- Never leave an empty block; write `pass`.
- If a name does not exist in the builtins table or in your source, it does not
  exist in NEX.
