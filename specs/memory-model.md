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

## References (stage 2, partial)

- `&T` and `&mut T` reference types, the borrow expressions `&place` /
  `&mut place`, and explicit dereference `*r` (reads and `*r = v` writes).
  References compile to a single pointer; an out-parameter `fn f(r: &mut i64)`
  reads and writes the caller's value through that pointer.
- A statement-scoped borrow checker rejects aliasing conflicts (`NXS053`):
  two `&mut` of the same place, or a shared `&` while a place is mutably
  borrowed, within one statement. It also rejects `&mut` of an immutable
  (`let`) binding (`NXS054`), so a mutable borrow always targets a `var`.
- Not yet implemented: lifetimes (a borrow outliving its referent), borrowing
  growable slices (`&[T]`), loading or assigning a whole aggregate through a
  reference, and returning references. Borrows are ephemeral and do not yet
  prevent a move across statements.

## Aggregate field/element access through a reference

- `(*r).field` works for `r: &Struct` / `&mut Struct` and `(*r)[i]` for
  `r: &[T; N]` (fixed arrays): both lower to a field/element access on the
  address the reference holds, so no whole-struct or whole-array load is
  emitted.
- Writes work too: `(*r).field = v` GEPs to the field and stores, and
  `*r = other` stores a whole struct value into the referenced location.
- Growable slices and fixed arrays: `(*r)[i]` reads and `(*r)[i] = v` writes
  work for `r: &[T]` and `r: &mut [T; N]`. For a growable array the reference
  points at a slot holding the header pointer, so the element write loads the
  header then stores through it.
- Not supported yet: loading an entire aggregate as a scalar value through a
  reference, and returning a reference to a local (`NXS055`).

## Automatic RAII for growable arrays

- A local growable array `[T]` is a slot holding a heap header. At entry the
  slot is initialised to a fresh empty array, and it is freed automatically
  before every `ret` (the IR builder appends the frees to each `Ret` block).
- Ownership is tracked per slot: a slot owns its buffer until it is assigned a
  value that is a bare identifier (a borrow), at which point it stops owning.
  Only owning slots are freed, so a growable-array parameter (owned by the
  caller) is never freed by the callee.
- Freeing the previous value before a store (reassignment or the next loop
  iteration) keeps repeated allocations leak-free without double-frees.
- `drop(x)` frees eagerly and leaves a fresh empty header in `x` so the scope
  exit does not free a dangling pointer.
- Returning an owned array transfers ownership to the caller (it is not freed).
- `string` is deliberately not auto-freed: string literals are static, and
  NEX does not yet distinguish static from heap strings.
- Self-assignment `x = x` and parallel assignment `a, b = b, a` keep ownership
  on the slot instead of freeing the buffer that is being re-stored, so they
  are not use-after-free.
- Not yet implemented: `Drop` trait / user-defined destructors, and `string`
  auto-free.

## Structs that own growable arrays

- A struct field of type `[T]` is an owning header pointer, exactly like a
  local growable array. Copying a struct (`var b = a`, passing by value,
  returning by value, assigning a whole struct) performs a **deep clone** of
  every `[T]` field via `nex_rt_array_clone`, and recurses into nested struct
  fields. The source struct keeps its buffers; the copy gets independent ones,
  so `push(b.xs, 9)` never changes `a.xs`.
- A struct parameter is therefore a real copy too: the callee owns independent
  array buffers and frees them at return, so mutating `param.field` inside the
  function is invisible to the caller.
- Reassigning a struct field of type `[T]` (`b.xs = other`) deep-clones the
  source array into the field and frees the old field buffer, so `other` stays
  valid and the field never aliases it. Reassigning a struct-typed field
  (`o.inner = other`) deep-clones the nested struct the same way.
- Parallel assignment of structs (`a, b = b, a`) reads both sources into
  independent temporaries before writing either target, so the swap is safe and
  the old buffers are freed.
- A local struct owns its `[T]` fields and frees them before every `ret`,
  including early returns and nested scopes (`owned_structs` RAII). Returning
  a struct transfers ownership of its arrays to the caller instead of freeing
  them.
- `drop(s.field)` on a growable struct field frees the buffer and resets the
  field to a fresh empty header, so a later scope-exit free cannot double-free.
- `drop(s)` on a whole struct local frees every `[T]` field (recursing into
  nested structs) and resets each to a fresh empty header. Like `drop(x)` on an
  array, it consumes the binding: a later use is `NXS050`.
- Not yet implemented: reference cycles between structs, and user-defined
  destructors for struct fields of other owning types.

## Rules for future implementation

- Safe code must not allow use-after-free or out-of-bounds access.
- Allocation behavior must be documented and measurable.
- Systems profile features must not silently change the semantics of safe code.
- Performance claims must be backed by benchmarks.

Until the ownership model lands, treat NEX as a systems language with manual
memory management and a small, documented runtime.
