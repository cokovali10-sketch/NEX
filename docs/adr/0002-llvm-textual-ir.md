# ADR 0002: Use textual LLVM IR and Clang

## Status

Accepted.

## Decision

Emit textual LLVM IR and compile it with Clang instead of linking LLVM libraries
directly.

## Rationale

This keeps the build simple and portable in environments where LLVM development
libraries are not installed, while still using LLVM optimizations and native code
generation.
