# Type System

NEX 1.0 has a static type system with move semantics for strings and arrays.

- Scalars: `i64`, `f64`, `bool`, `string`
- Structs: named tuples with named fields
- Enums: C-like enums with payloads planned for future
- Arrays: growable `[T]` and fixed-size `[T; N]`
- Tuples: `(T, U, ...)`
- Generics: monomorphized generics for scalar types only
- Option: `Option<T>` for nullable values
- Result: `Result<T, E>` for error propagation