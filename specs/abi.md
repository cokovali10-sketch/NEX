# NEX ABI (1.0 Status)

## Implemented

NEX 1.0 can import C functions with `extern "C" fn`. It does not define a stable
ABI for exporting NEX functions, structs, or callbacks. Generated programs use
the platform C ABI for runtime calls such as printing.

## Function lowering

- User functions are emitted with the mangled name `nex.fn.<name>`.
- `main` is wrapped by a C-compatible `main` function.
- Primitive types map to C-compatible scalar types.
- `string` is currently represented as a pointer to a NUL-terminated UTF-8
  byte array.

## Planned

- Stable C ABI export and import.
- Struct layout rules.
- Callback support.
- Versioned runtime ABI.
- Adapter SDK for other languages.
