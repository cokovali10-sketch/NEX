# NEX Type System (0.1)

## Implemented types

- `bool`
- `char` (Unicode scalar value, lowered as `i32`)
- `i8`, `i16`, `i32`, `i64`
- `u8`, `u16`, `u32`, `u64`
- `f32`, `f64`
- `string`
- `void` (function return only)

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
- `and` and `or` require `bool`.
- `not` requires `bool`.
- Unary `-` requires a signed integer or floating point type.

## Explicit casts

`as` can convert between numeric types, `bool`, and `char`:

```nex
let x = 42 as f64
let y = 3.9 as i64
let z = true as i64
```

## Planned

- Generic type parameters.
- Struct, enum, tuple, array, optional, and result types.
- Traits and type constraints.
