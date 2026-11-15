# NEX Language Specification (0.2 Status)

This document describes the intended NEX language. Features marked as
**implemented in 0.2** are available in the current compiler. Other features are
planned and are not yet implemented.

## Implemented in 0.2

- Function declarations with parameters and optional return type.
- `let` (immutable) and `var` (mutable) bindings with type inference.
- Assignment to mutable bindings.
- Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=` (desugared to
  `x = x <op> value`).
- Integer `i64`, floating point `f64`, `bool`, `char`, and `string` types.
- Numeric types `i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `u64`, `f32` in
  annotations and casts through literal coercion.
- Arithmetic: `+`, `-`, `*`, `/`, `%`.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`.
- Logical: `and`, `or`, `not`.
- Unary negation.
- `if`, `elif`, `else`.
- `while`.
- `for ... in ... .. ...`.
- `return`, `break`, `continue`.
- Function calls.
- Built-in `print` and `println`.
- String comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`) comparing contents.
- Struct declarations with scalar fields, construction, field access, and
  field assignment (see `specs/types.md`).
- Enum declarations and `match` over enum values.
- Array literals and indexing reads/writes.
- Tuple literals and positional field access.
- Nested modules (`module name:`), `use` aliases, and `pub` visibility.
- Explicit casts with `as`.
- Top-level `const` declarations with compile-time constant initializers.

## Planned

- Generics and type constraints.
- Traits and pattern matching beyond enums.
- Struct constants.
- Arrays and tuples in type annotations, parameters, and return values.
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
- Embedded NUL characters in strings are not supported in 0.2.

## Program structure

A program is a sequence of struct, constant, and function declarations.
`fn main()` is required.

```nex
fn main():
    print("Hello from NEX!")
```

## Types

See `specs/types.md`.

## Memory model

See `specs/memory-model.md`.
