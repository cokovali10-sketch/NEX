# Interoperability

NEX can call C functions and use C libraries via `extern "C"`.

## C FFI
```nex
extern "C" fn puts(msg: string) -> i64
extern "C" fn getpid() -> i64
```

## Calling C
- Pass strings as `string` (NEX copies to C-compatible null-terminated)
- Pass integers, floats, booleans directly
- Return types: `i64`, `f64`, `bool`, `string` supported
- C functions must be declared with `extern "C"`

## Using NEX from C
- Compile NEX to a shared library with `--emit obj` and link manually
- NEX programs can be called as C functions if they follow C ABI

## Limitations
- No C structs or unions yet
- No callback functions from C to NEX
- No C pointer arithmetic in NEX