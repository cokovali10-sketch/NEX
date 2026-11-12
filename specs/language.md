# NEX Language Specification (0.1 Status)

This document describes the intended NEX language. Features marked as
**implemented in 0.1** are available in the current compiler. Other features are
planned and are not yet implemented.

## Implemented in 0.1

- Function declarations with parameters and optional return type.
- `let` (immutable) and `var` (mutable) bindings with type inference.
- Assignment to mutable bindings.
- Integer `i64`, floating point `f64`, `bool`, `char`, and `string` types.
- Numeric types `i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `u64`, `f32` in
  annotations and casts through literal coercion.
- Arithmetic: `+`, `-`, `*`, `/`, `%`.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`.
- Logical: `and`, `or`, `not`.
- Unary negation.
- `if`, `elif`, `else`.
- `while`.
- `return`, `break`, `continue`.
- Function calls.
- Built-in `print` and `println`.
- Explicit casts with `as`.

## Planned

- Generics and type constraints.
- Structs, enums, traits, and pattern matching.
- Arrays, tuples, and optional/result types.
- Modules and imports.
- Async/await and structured concurrency.
- Macros and compile-time evaluation.
- Unsafe blocks and low-level pointers.
- Expanded standard library.

## Lexical rules

- Source files use UTF-8.
- Indentation is significant and uses four spaces per level.
- Tabs in leading indentation are errors.
- `#` starts a line comment.
- Strings support `\n`, `\r`, `\t`, `\\`, `\"`, `\xNN`, and `\u{...}`.
- Embedded NUL characters in strings are not supported in 0.1.

## Program structure

A program is a sequence of function declarations. `fn main()` is required.

```nex
fn main():
    print("Hello from NEX!")
```

## Types

See `specs/types.md`.

## Memory model

See `specs/memory-model.md`.
