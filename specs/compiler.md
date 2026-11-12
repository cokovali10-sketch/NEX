# NEX Compiler Architecture

## Stages

1. Lexer: reads source text and produces tokens with spans.
2. Parser: builds the AST.
3. Semantic analyzer: resolves scopes and checks types.
4. IR builder: lowers typed AST to NEX IR.
5. Backend: emits LLVM textual IR.
6. Toolchain: invokes Clang to compile and link.

## Diagnostics

Diagnostics use stable codes:

- `NXLxxx`: lexer.
- `NXPxxx`: parser.
- `NXSxxx`: semantic analysis.
- `NXCxxx`: compiler and code generation.

## Build profiles

- Debug: `-O0 -g`.
- Release: `-O2`.
- Max: `-O3`.

## Output kinds

- Executable.
- Object file.
- LLVM IR.
- Check only.
