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
- [x] Struct parameters and return values.
- [x] Nested struct fields.
- [x] Enforce `pub` visibility across modules.
- [x] Array and tuple type annotations, parameters, and return values.
- [x] Traits and pattern matching beyond enums.
- [x] Better control-flow analysis and definite-return checking for constant conditions.
- [x] Richer diagnostics with help suggestions.

## NEX 0.3 - Native Compiler

- [x] Constant folding in NEX IR.
- [x] Debug symbols via Clang (`-g`).
- [ ] Cross-platform targets.
- [x] Reproducible build metadata (build.json).

## NEX 0.4 - Memory and Safety

- [ ] Ownership and borrowing model.
- [ ] Lifetime analysis.
- [ ] Allocators and arenas.
- [x] Bounds checking for array indexing.
- [ ] Unsafe blocks and sanitizer integration.

## NEX 0.5 - Standard Library

- [ ] Collections, math, algorithms (design fixed: see `specs/dynamic-arrays.md`).
- [x] String functions: len, concat, str_contains, str_index_of,
      str_substring, str_upper, str_lower, str_trim, to_string.
- [ ] Filesystem and networking.
- [ ] Time, logging, process APIs.

## NEX 0.6 - Interoperability

- [x] C ABI import (`extern "C" fn`).
- [ ] Foreign function declarations.
- [ ] Adapter SDK.
- [ ] Python and Rust adapters.

## NEX 0.7 - Developer Tools

- [x] Formatter (`nex fmt`, AST-based, `--check`/`--write`).
- [x] Linter (`nex lint`: unused bindings, unused private functions,
      self-comparisons, constant conditions, unused imports).
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
