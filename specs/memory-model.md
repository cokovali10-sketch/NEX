# NEX Memory Model (0.1 Specification)

The full memory model is not implemented in NEX 0.1. This document records the
intended design so that later work remains consistent.

## Goals

- Predictable performance.
- Safety in normal code.
- Explicit control for systems code.
- Low overhead.

## Implemented in 0.1

- Value semantics for local variables.
- Stack allocation through LLVM `alloca`.
- No heap allocator.
- No garbage collection.
- No ownership or lifetime checker.

## Planned model

- Immutable bindings by default.
- Explicit mutable bindings.
- Ownership for heap-allocated values.
- Borrowing and lifetime analysis.
- RAII for resources.
- Arena allocation.
- Custom allocators in NEX Systems profile.
- Explicit `unsafe` blocks for low-level operations.
- Bounds checking for arrays and slices.

## Rules for future implementation

- Safe code must not allow use-after-free or out-of-bounds access.
- Allocation behavior must be documented and measurable.
- Systems profile features must not silently change the semantics of safe code.
- Performance claims must be backed by benchmarks.
