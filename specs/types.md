# NEX Type System (0.1)

## Implemented types

- `bool`
- `char` (Unicode scalar value, lowered as `i32`)
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `string`
- `void` (function return only)
- User-defined `struct` with scalar fields (see below)
- User-defined `enum` (lowered to an `i32` tag)

## Type inference

- Integer literals default to `i64`.
- Floating point literals default to `f64`.
- Boolean literals are `bool`.
- String literals are `string`.
- Character literals are `char`.
- `let` without an annotation infers from the initializer.
- `var` without an annotation infers from the initializer.

## Coercion

NEX 0.1 intentionally avoids implicit widening. The only implicit conversions are:

- An integer literal may be coerced to an integer type if it fits in that type.
- A floating point literal may be coerced to `f32` or `f64`.

For example, `let x: u8 = 255` is valid, but `let x: u8 = y` where `y: i64`
requires an explicit conversion.

## Operators

- Arithmetic requires both operands to have the same numeric type.
- Comparisons require both operands to have the same type.
- `==` and `!=` are defined for `bool`, `char`, numeric types, and `string`
  (string equality compares contents, not pointers).
- `<`, `<=`, `>`, and `>=` are defined for `bool`, `char`, numeric types, and
  `string` (lexicographic ordering by UTF-8 bytes).
- `and` and `or` require `bool`.
- `not` requires `bool`.
- Unary `-` requires a signed integer or floating point type.

## User-defined structs

```nex
struct Point:
    x: i64
    y: i64
```

- Fields are declared as `name: type`, one per line.
- A struct is constructed positionally: `Point(1, 2)`.
- Fields are read with `p.x` and assigned with `p.x = value` (including the
  compound assignment operators) when the binding was declared with `var`.
- Field types must be scalar: `bool`, `char`, the numeric types, or `string`.
  Nested struct fields are planned.
- A struct value can be copied by initializing a binding from an existing
  struct variable or constructor, for example `let q = p`.
- Struct values cannot be printed, passed to functions, returned from
  functions, declared as constants, or compared with `==`/`!=` yet.

## User-defined enums

```nex
enum Color:
    Red
    Green
    Blue
```

- Variant names must be unique across all enums in a program.
- An enum value is written `Color.Red` or, when unambiguous, `Red`.
- Enums are lowered to `i32` tags, so equality (`==`, `!=`) works.
- `match` over an enum value supports `case Variant:` arms and an optional
  trailing `else:` arm. A non-exhaustive match without `else` is a warning.
- Enum values cannot be printed or cast.

## Explicit casts

`as` can convert between numeric types, `bool`, and `char`:

```nex
let x = 42 as f64
let y = 3.9 as i64
let z = true as i64
```

## Planned

- Generic type parameters.
- Structs with nested struct fields, struct parameters and return values, and
  struct constants.
- Tuple, array, optional, and result types.
- Traits and type constraints.
