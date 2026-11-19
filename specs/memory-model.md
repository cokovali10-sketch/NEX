# NEX Memory Model (1.0 Status)

NEX 1.0 has a simple, explicit memory model. It is **not** a safe memory model
yet: there is no ownership, borrowing, or lifetime checker. This document
records what the compiler and runtime actually do today, and what remains
planned.

## Goals

- Predictable performance.
- Safety in normal code (long-term goal).
- Explicit control for systems code.
- Low overhead.

## Implemented in 1.0

- Immutable `let` bindings and explicit mutable `var` bindings.
- Value semantics for scalars, structs, fixed arrays, and tuples: assignment
  and parameter passing copy the value.
- Stack allocation through LLVM `alloca`.
- Heap allocation through the C runtime (`malloc`/`realloc`) for strings,
  growable arrays (`[T]`), maps, JSON trees, the string builder, and the
  thread pool.
- No garbage collector and no ownership checker. Heap values are not freed
  automatically; long-running programs that build many strings or arrays can
  leak memory.
- Bounds checking for fixed and growable array indexing; out-of-range reads
  panic. String indexing returns an empty string and string slicing clamps
  bounds.
- `unsafe:` blocks group low-level operations (`alloc`, `free`, `peek`,
  `poke`) but do not disable type checking.
- Raw allocation helpers `alloc`/`free` and `i64` load/store helpers
  `peek`/`poke`.

## Planned model

- Ownership for heap-allocated values.
- Borrowing and lifetime analysis.
- RAII for resources.
- Arena allocation.
- Custom allocators in a NEX Systems profile.
- A safe slice type with explicit lifetime relationships.

## Rules for future implementation

- Safe code must not allow use-after-free or out-of-bounds access.
- Allocation behavior must be documented and measurable.
- Systems profile features must not silently change the semantics of safe code.
- Performance claims must be backed by benchmarks.

Until the ownership model lands, treat NEX as a systems language with manual
memory management and a small, documented runtime.
