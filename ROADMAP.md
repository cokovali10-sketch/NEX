# NEX Roadmap

## NEX 0.1 - Language Foundation (complete)

- [x] Rust workspace and repository governance.
- [x] Lexer with indentation, comments, literals, and diagnostics.
- [x] Parser with functions, bindings, control flow, expressions.
- [x] AST and semantic analysis.
- [x] Type inference for local bindings and literals.
- [x] NEX IR.
- [x] LLVM textual IR backend.
- [x] Native executable generation on Windows.
- [x] `nex` and `nexc` commands.
- [x] End-to-end tests.

## NEX 0.2 - Semantic Core (current)

- [x] Structs: declarations with scalar fields, construction, field access,
      field assignment, and struct copies by binding initialization.
- [x] Enums and `match` over enum values (case arms and `else` arm).
- [x] Arrays: literals, indexing reads and writes.
- [x] Tuples.
- [x] Modules and imports (nested `module` blocks and `use` aliases).
- [ ] Nested struct fields.
- [ ] Struct parameters and return values.
- [ ] Enforce `pub` visibility across modules.
- [ ] Arrays and tuples in type annotations, parameters, and return values.
- [ ] Traits and pattern matching beyond enums.
- [ ] Better control-flow analysis and definite-return checking.
- [ ] Richer diagnostics with fixes and suggestions.

## NEX 0.3 - Native Compiler

- [ ] Optimization passes in NEX IR.
- [ ] Debug information.
- [ ] Cross-platform targets.
- [ ] Reproducible build metadata.

## NEX 0.4 - Memory and Safety

- [ ] Ownership and borrowing model.
- [ ] Lifetime analysis.
- [ ] Allocators and arenas.
- [ ] Bounds checking.
- [ ] Unsafe blocks and sanitizer integration.

## NEX 0.5 - Standard Library

- [ ] Collections, strings, math, algorithms.
- [ ] Filesystem and networking.
- [ ] Time, logging, process APIs.

## NEX 0.6 - Interoperability

- [ ] C ABI import/export.
- [ ] Foreign function declarations.
- [ ] Adapter SDK.
- [ ] Python and Rust adapters.

## NEX 0.7 - Developer Tools

- [ ] Formatter and linter.
- [ ] Debugger.
- [ ] Profiler.
- [ ] Package manager.

## NEX 0.8 - NEX Studio

- [ ] Editor, project explorer, terminal.
- [ ] LSP integration.
- [ ] Debugger and profiler integration.

## NEX 0.9 - Stabilization

- [ ] Security testing.
- [ ] Performance benchmarks.
- [ ] Cross-platform CI.
- [ ] Documentation completion.

## NEX 1.0 - Stable Release

- [ ] Stable language specification.
- [ ] Stable compiler and runtime.
- [ ] Installer and release artifacts.
- [ ] First stable integrations.
