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