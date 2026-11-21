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
- Move semantics (stage 1) for the heap-owning types `string` and growable
  arrays `[T]`: binding such a value to another variable moves ownership, and
  any later use of the moved-from variable is a `NXS050` error. `clone(value)`
  is the opt-in explicit deep copy that borrows its argument and leaves the
  source usable. The move
  checker is a control-flow-aware analysis: a binding is considered moved after
  an `if` only when it is moved on every path (including the `else`), so there
  are no false positives, and moves inside a loop or a single branch (which may
  not run) do not leak out. Cross-path use-after-move that is not a definite
  move is left for the borrow-checker stage.
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

- Borrowing (`&T` / `&mut T`) and lifetime analysis, building on the stage-1
  move checker.
- Automatic `Drop` / RAII for heap values (deterministic destruction), which
  turns the current copy-based codegen for moved values into true pointer
  moves.
- Custom allocators (`@arena`, `@pool`, optional `@gc`) in a NEX Systems
  profile. Arena bump allocators already exist as builtins.
- A safe slice type with explicit lifetime relationships.

## Rules for future implementation

- Safe code must not allow use-after-free or out-of-bounds access.
- Allocation behavior must be documented and measurable.
- Systems profile features must not silently change the semantics of safe code.
- Performance claims must be backed by benchmarks.

Until the ownership model lands, treat NEX as a systems language with manual
memory management and a small, documented runtime.
