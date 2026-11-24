# Compiler Architecture

NEX is a multi-pass compiler written in Rust:

1. **Lexer** (`nex-lexer`): tokenizes source code
2. **Parser** (`nex-parser`): builds AST from tokens, with statement-level recovery
3. **Semantic Analysis** (`nex-semantic`): type-checking, move/borrow checking, diagnostics
4. **IR Builder** (`nex-ir`): generates LLVM IR from AST
5. **Backend** (`nex-backend`): compiles IR to native code via LLVM

The compiler includes a large-stack mode to handle deeply nested code without stack overflow.