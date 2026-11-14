# Changelog

## 0.1.0 - 2026-11-12

### Added

- Struct declarations with scalar fields, construction, field access, and
  field assignment.
- String equality and inequality (`==`, `!=`) with content comparison.
- String ordering (`<`, `<=`, `>`, `>=`) using lexicographic comparison.
- Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=`.
- Explicit `as` casts between numeric types, `bool`, and `char`.
- Top-level `const` declarations with compile-time constant initializers.
- `for ... in ... .. ...` loops.
- Initial NEX workspace and repository governance files.
- Indentation-aware lexer with diagnostics.
- Recursive-descent parser for functions, bindings, control flow, and expressions.
- AST and semantic analysis with type inference for a small subset.
- NEX IR and LLVM textual IR backend.
- Native executable generation through Clang.
- `nex` project CLI and `nexc` compiler driver.
- End-to-end tests for compilation and execution.
- Initial language, grammar, type, memory, compiler, and diagnostics specifications.

### Known limitations

- Only Windows x86-64 is tested.
- No generics, enums, modules, arrays, or standard library yet.
- Struct fields must be scalar types; struct values cannot be passed to
  functions, returned, printed, or used as constants yet.
- No ownership or lifetime checking yet.
- `nex fmt`, `nex lint`, `nex add`, and `nex benchmark` are not implemented.
- `nex` arithmetic is limited to `i64`, `f64`, `bool`, `char`, and `string`.
