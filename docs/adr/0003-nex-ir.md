# ADR 0003: Maintain a separate NEX IR

## Status

Accepted.

## Decision

Lower typed AST to NEX IR before emitting LLVM IR.

## Rationale

NEX IR preserves language-level structure and keeps the frontend independent
from LLVM. It also provides a place for future optimizations and backends.
