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

## NEX 0.2 - Semantic Core (complete)

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
- [x] Cross-platform target validation (`--target <triple>`, `nex doctor --target`).
- [x] Reproducible build metadata (build.json).

## NEX 0.4 - Memory and Safety

- [~] Ownership and borrowing model (stage 1 move checker + RAII for `[T]`
      locals and struct fields, plus deep-clone value semantics for struct copy/
      assign/param/return and `[Struct]` arrays; full borrow/lifetime analysis
      still open).
- [ ] Lifetime analysis.
- [x] Allocators and arenas.
- [x] Bounds checking for array indexing.
- [x] Unsafe blocks for grouping low-level operations.
- [x] Sanitizer integration.

## NEX 0.5 - Standard Library

- [x] Collections, math, algorithms (design fixed: see `specs/dynamic-arrays.md`).
- [x] String functions: len, concat, str_contains, str_index_of,
      str_substring, str_upper, str_lower, str_trim, to_string.
- [x] Filesystem and blocking networking (TCP/HTTP helpers).
- [x] Time and concurrency APIs (threads, mutexes, thread pool).
- [x] Logging and process APIs.

## NEX 0.6 - Interoperability

- [x] C ABI import (`extern "C" fn`).
- [x] Foreign function declarations.
- [ ] Adapter SDK.
- [ ] Python and Rust adapters.

## NEX 0.7 - Developer Tools

- [x] Formatter (`nex fmt`, AST-based, `--check`/`--write`).
- [x] Linter (`nex lint`: unused bindings, unused private functions,
      self-comparisons, constant conditions, unused imports).
- [x] `nex debug` / `nex profile` build-and-run commands.
- [ ] Full debugger frontend (breakpoints, stepping, variable inspection).
- [ ] Sampling profiler.
- [x] Local dependency vendoring (`nex add`).

## NEX 0.8 - NEX Studio

- [x] Standalone desktop MVP: project explorer, syntax-highlighted editor,
      file tabs, create/save, unsaved-change prompts, and background build/run.
- [x] Windows folder picker, recent projects, nested file/folder creation, and project-wide search.
- [x] Background compiler diagnostics with clickable file and line navigation.
- [ ] LSP integration in the editor: completion, hover, and navigation.
- [ ] Terminal panel.
- [x] LSP integration (`nex lsp`: hover, go-to-definition, completion, document symbols, formatting, diagnostics).
- [ ] Debugger and profiler integration.

## NEX 0.9 - Stabilization

- [ ] Security testing.
- [x] Performance benchmarks (`nex benchmark`).
- [x] Cross-platform CI.
- [x] Documentation completion.

## NEX 1.0 - Stable Release (v1.0.1)

- [x] Stable language specification (see `specs/`).
- [x] Stable compiler and runtime for the implemented feature set.
- [x] Installer scripts (`install.ps1`, `install.sh`).
- [x] Published release binaries and archives (Windows and Linux, with SHA-256 checksums).
- [x] Release checks green at the `v1.0.1` tag (fmt, Clippy, tests on Windows and Ubuntu).
- [ ] First stable integrations.

## Post-1.0 (planned)

- Full ownership, borrowing and lifetime analysis.
- Broader platform coverage (macOS, ARM64, cross-compilation).
- NEX Studio (editor, project explorer, LSP integration).
- Foreign adapters for Python and Rust.
- Full debugger and sampling profiler integration.
- Generic type arguments in annotations and generic constraints.
