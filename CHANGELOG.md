# Changelog

## 0.1.0 - 2026-11-12

### Added

- Nested modules (`module name:`) and `use` aliases.
- `pub` visibility for module items, enforced on access and `use`.
- Tuple literals and positional field access.
- Array literals and indexing reads/writes.
- Runtime bounds checking for array indexing.
- Array and tuple type annotations, parameters, and return values.
- Enum declarations and `match` over enum values.
- Struct declarations with scalar fields, construction, field access, and
  field assignment.
- Printing for struct, array, and tuple values.
- Struct, array, and tuple constants.
- Equality and inequality for struct, array, and tuple values.
- Nested struct fields, including reads, copies, and assignment.
- Struct parameters and return values by value.
- String equality and inequality (`==`, `!=`) with content comparison.
- String ordering (`<`, `<=`, `>`, `>=`) using lexicographic comparison.
- Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=`.
- Explicit `as` casts between numeric types, `bool`, and `char`.
- Top-level `const` declarations with compile-time constant initializers.
- `for ... in ... .. ...` loops.
- Definite-return checking for `while true` and constant `if` conditions.
- Initial NEX workspace and repository governance files.
- Indentation-aware lexer with diagnostics.
- Recursive-descent parser for functions, bindings, control flow, and expressions.
- AST and semantic analysis with type inference for a small subset.
- NEX IR and LLVM textual IR backend.
- Native executable generation through Clang.
- `nex` project CLI and `nexc` compiler driver.
- External C function declarations and calls.
- Built-in `len` and `abs`.
- Built-in `min`, `max`, `sqrt`, and `pow`.
- Constant folding for compile-time constant expressions.
- End-to-end tests for compilation and execution.
- Initial language, grammar, type, memory, compiler, and diagnostics specifications.
- Help suggestions in diagnostic output for common errors.

### Known limitations

- Only Windows x86-64 is tested.
- No generics, traits, or standard library yet.
- Struct fields must be scalar types; struct values cannot be printed or used
  as constants yet.
- Arrays and tuples cannot yet be used as parameter or return types.
- No ownership or lifetime checking yet.
- `pub` declarations are parsed but visibility is not enforced yet.
- `nex fmt`, `nex lint`, `nex add`, and `nex benchmark` are not implemented.
- `nex` arithmetic is limited to numeric types, `bool`, `char`, and `string`.
