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

## Memory model in 0.1

NEX 0.1 uses stack allocation and value semantics. Local variables are lowered to
`alloca` instructions. There is no heap allocator, ownership checker, or garbage
collector in this milestone. The memory model is specified in
`specs/memory-model.md` and will be implemented incrementally.

## Runtime

Generated programs link a small C runtime, `runtime/core/nex_rt.c`. In 0.1 it
provides printing and panic support. The compiler embeds this source at build
time, so the toolchain does not depend on an installed NEX runtime.

## Toolchain

Clang is discovered through `NEX_CLANG`, `PATH`, and common installation
locations. The driver emits textual LLVM IR and asks Clang to compile and link it.
This avoids a hard dependency on LLVM development libraries while still using LLVM
optimizations.
