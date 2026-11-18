# NEX Language Specification (1.0 Status)

This document describes the intended NEX language. Features marked as
**implemented in 1.0** are available in the current compiler. Other features are
planned and are not yet implemented.

## Implemented in 1.0

- Function declarations with parameters and optional return type.
- External C function declarations with `extern "C" fn`.
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
- Built-in `len` for strings and arrays, `abs` for `i64`, `min`/`max` for
  `i64` and `f64`, and `sqrt`/`pow` for `f64`.
- Built-in `assert` and `panic`.
- Built-in `alloc` and `free` for raw memory.
- Built-in `peek` and `poke` for raw `i64` memory access.
- Built-in `concat` and `to_string`.
- String comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`) comparing contents.
- String interpolation with `f"..."`.
- Struct declarations with scalar fields, construction, field access, and
  field assignment (see `specs/types.md`).
- Enum declarations and `match` over enum values.
- `match` over integers, booleans, characters, strings, with guards and ranges.
- Trait declarations, `impl Trait for Type`, and static-dispatch method calls.
- Array literals and indexing reads/writes.
- Tuple literals and positional field access.
- Nested modules (`module name:`), `use` aliases, and `pub` visibility.
- File-based imports (`use alias from "file.nex"`).
- Explicit casts with `as`.
- Type aliases (`type Name = Type`).
- `unsafe:` blocks for low-level operations.
- Generic functions and structs (monomorphization).
- Top-level `const` declarations with compile-time constant initializers.

## Planned

- Generics and type constraints.
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

## Traits

Traits declare method signatures. Implementations are static-dispatched to the
concrete struct type:

```nex
trait Shape:
    fn area(self) -> f64

struct Rect:
    width: f64
    height: f64

impl Shape for Rect:
    fn area(self) -> f64:
        return self.width * self.height
```

Method calls use `receiver.method(...)` syntax and resolve to the registered
`Type.method` symbol at compile time.

## Pattern matching

`match` supports enum variants, integer, boolean, character, and string
patterns. Scalar matches require an `else` arm.

```nex
match value:
    case 1:
        print("one")
    case 2:
        print("two")
    else:
        print("other")
```

## Memory model

See `specs/memory-model.md`.
