# NEX Architecture

NEX is implemented as a Rust workspace. The compiler is split into small crates so
that each stage can be tested independently and reused by both `nexc` and `nex`.

## Compilation pipeline

```text
.nex source
  -> nex-lexer
  -> nex-parser
  -> nex-ast
  -> nex-semantic
  -> nex-ir
  -> nex-backend (LLVM textual IR)
  -> clang
  -> native executable
```

## Crates

- `nex-diagnostics`: spans, diagnostics, source files, rendering.
- `nex-lexer`: indentation-aware lexer. Tabs in leading indentation are rejected.
- `nex-ast`: syntax tree without type information.
- `nex-parser`: recursive-descent parser with statement-level recovery.
- `nex-semantic`: scope resolution, type checking, type inference, diagnostics.
- `nex-ir`: NEX's own intermediate representation and AST-to-IR lowering.
- `nex-backend`: LLVM textual IR emission and Clang toolchain integration.
- `nex-driver`: full pipeline used by `nexc` and `nex`.
- `nex-cli`: project-oriented command-line interface.

## Why NEX has its own IR

NEX does not lower source code directly to LLVM IR. Semantic information is
first lowered to `nex-ir`, which is independent of LLVM. This keeps the frontend
portable and leaves room for future backends, optimization passes, and a stable
debug representation.

## Memory model in 1.0

NEX uses stack allocation and value semantics for scalars, structs, fixed
arrays, and tuples. Local variables are lowered to `alloca` instructions.
Strings, growable arrays, maps, JSON trees, the string builder, and the thread
pool are allocated through the C runtime heap (`malloc`/`realloc`). There is no
ownership checker and no garbage collector, so heap values are not freed
automatically. The model is documented in `specs/memory-model.md`.

## Runtime

Generated programs link a small C runtime, `runtime/core/nex_rt.c`. In 1.0 it
provides printing and panic support, strings, files, blocking TCP/HTTP helpers,
JSON, maps, threads and mutexes, a thread pool, a router, a string builder, time
and math functions. The compiler embeds this source at build time, so the
toolchain does not depend on an installed NEX runtime.

## Toolchain

Clang is discovered through `NEX_CLANG`, `PATH`, and common installation
locations. The driver emits textual LLVM IR and asks Clang to compile and link it.
This avoids a hard dependency on LLVM development libraries while still using LLVM
optimizations.
