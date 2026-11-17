# Dynamic Arrays — Design Decision

Status: accepted (2026-11-15). This document fixes the design before
implementation starts, so the compiler changes can be made in one pass instead
of being rediscovered in each layer.

## Implementation note (2026-11-17)

The design below was adjusted while implementing it, and the code is the source
of truth:

- `Type` **keeps `Copy`**. A growable array is `Type::DynArray(&'static str)`,
  where the string is the interned display name (for example `[i64]`). The
  element type is read from the existing `Analysis::arrays` registry, which
  already stores `elem: Type`. This avoided changing every `match` on `Type`.
- In IR a growable array value is a **plain pointer** to the header
  `{ ptr, i64, i64 }`, so a variable costs one word, not three.
- Field access uses a dedicated `Inst::HeaderPtr` with `i32` indices. The
  installed clang rejects `getelementptr` with `i64` struct indices.
- `push` calls `nex_rt_array_ensure`, which doubles the capacity
  (`4, 8, 16, ...`) and reallocs only the data buffer.

Everything else in this document (semantics, bounds checks, aliasing) holds.

## Why

Every collection NEX will ever need — `json`, `str_split`, buffers, result sets
from a database, a router table — depends on a growable array. Today NEX has only
fixed-size arrays `[T; N]`, whose length is part of the type and must be known at
compile time. That makes the language unable to express "read N items from
somewhere".

## Scope

In scope for this decision:

- growable arrays of any element type, written `[T]`;
- literals, indexing, `len`, `push`, and iteration with `for`;
- bounds checking on every read and write;
- passing arrays to functions and returning them.

Out of scope (separate decisions): maps/dictionaries, string views and slicing,
sorting and other algorithms, `str_split` itself (it becomes possible once this
lands), and any ownership or lifetime analysis.

## Surface syntax

```nex
var names = ["ann", "bob"]        # [string], inferred
var empty = [i64]                 # growable, empty
var buffer = [u8]                 # byte buffer

names.push("cid")
print(len(names))                 # 3
print(names[0])                   # ann
names[1] = "bobby"
```

`[T; N]` stays exactly as it is: a fixed-size array is a value type and is not
affected by this change. `[T]` is a new, distinct type.

## Runtime representation

A growable array is a three-word value:

| Field | Type | Meaning |
|---|---|---|
| `data` | `*T` | heap buffer, `null` when empty |
| `len` | `i64` | number of initialized elements |
| `cap` | `i64` | allocated slots, `>= len` |

In LLVM IR the value is the struct `{ ptr, i64, i64 }`. It is returned and passed
by value (three words), which keeps the ABI simple and avoids a hidden out-pointer
parameter. No vtable, no header, no reference counting.

Rationale for a struct instead of a pointer to a heap object: a value type keeps
assignment and copy semantics identical to the fixed-size arrays NEX already has,
and the compiler can keep the struct in registers for small arrays.

## Allocation

- `push` grows by doubling (`cap = max(4, cap * 2)`) and uses `realloc`;
- an empty array allocates on the first `push`;
- `cap` never shrinks in this milestone;
- allocation failure is a `panic` with a message, not a null dereference, because
  the runtime already owns the allocator;
- there is no custom allocator API yet: NEX uses the process allocator, which is
  the same `malloc` the string functions use.

Memory is never freed in this milestone: NEX has no ownership model yet, so
arrays leak until the process exits. That is documented in `docs/memory-model.md`
and is the same trade-off already made for `str_repeat`, `concat`, and every
other allocating string function.

## Type system

This is the expensive part, and the reason the design must be fixed first.

Today `Type` is `Copy` and array types are two `&'static str` fields
(`display`, `llvm`). A growable array needs its element type:

```rust
DynArray {
    element: Box<Type>,
}
```

`Box<Type>` is not `Copy`, so `Type` must lose `Copy`. The migration is
mechanical but touches every match on `Type`:

| Layer | Change |
|---|---|
| `compiler/semantic/src/types.rs` | `Type` loses `Copy`; add `DynArray { element }`; add `is_scalar()`, `is_array()` helpers for the places that only need a yes/no answer |
| `compiler/semantic/src/lib.rs` | literal typing: a non-empty array literal infers `[T]`; an empty literal takes the annotation; `push` checks `element` and returns `void`; `len` accepts both array forms; indexing checks the element type |
| `compiler/ir/src/types.rs` | `IrType::DynArray` is **not** added: the IR keeps `Array(&'static str)` for the struct spelling `{ ptr, i64, i64 }` and adds a separate instruction for indexing |
| `compiler/ir/src/builder.rs` | new `Inst::DynIndexPtr { dst, base, elem_ty, index }` and `Inst::DynLen { dst, base }`; `push` lowers to `len`/`cap` loads, a conditional `realloc`, a GEP, and a store |
| `compiler/backend/src/llvm.rs` | emit `getelementptr` against `elem_ty`, load `len` and `cap` from the struct, call `realloc` |
| `runtime/core/nex_rt.c` | `nex_rt_array_grow(ptr, elem_size, cap)` returning a struct is awkward in C; instead expose `void *nex_rt_realloc_array(void *data, size_t new_bytes)` and let the IR do the bookkeeping |

`display` for a dynamic array is the element spelling in brackets (`[i64]`), and
`llvm` is always `{ ptr, i64, i64 }`. Element types that are themselves arrays or
structs are supported by the same representation; only the GEP element type
changes.

## Operations and semantics

| Operation | Behaviour |
|---|---|
| Literal `[a, b, c]` | allocates exactly 3 slots, `cap == len == 3` |
| `[T]` annotation | declares an empty growable array (`data = null`, `len = 0`, `cap = 0`) |
| `a[i]` read | bounds-checked: `0 <= i < len`, else `panic` with `index out of bounds` |
| `a[i] = v` write | same check, then store |
| `a.push(v)` | grows if `len == cap`, then stores at `len` and increments `len` |
| `len(a)` | returns the `len` field, no call |
| `for x in a` | iterates `0..len` by index (no iterator type in this milestone) |
| assignment `b = a` | copies the three words: both names share the same buffer. Documented, not hidden. |
| passing to a function | by value (three words) |
| returning from a function | by value |

Aliasing after assignment is deliberate: NEX has no ownership yet, so a copy is a
shallow copy. This is stated in `docs/memory-model.md` so it is not a surprise.

## Error handling

- out-of-bounds access is a runtime panic (`panic` builtin path), consistent with
  the existing fixed-size array behaviour;
- allocation failure panics with `out of memory`;
- `push` on a null array (declared with `[T]` but never pushed) works: the first
  push allocates.

## Tests required before the change is called done

1. literal type inference (`["a"]` is `[string]`, `[]` needs an annotation);
2. bounds check on read and on write;
3. `push` growth across a doubling boundary (0 → 1 → 4 → 8);
4. `len` after `push`;
5. assignment aliasing (mutating one name is visible through the other);
6. array of structs and array of arrays;
7. passing and returning arrays from functions;
8. `for` iteration over a growable array;
9. a panic on out-of-bounds access, plus the existing 92 tests staying green.

## Rollout

1. Land the `Type` change with `DynArray` unused, so the compiler still builds.
2. Add literal typing and the `DynArray` display spelling.
3. Add IR instructions and the backend, with no surface syntax yet.
22. Turn on `[T]` in the parser and the `push`/`len` builtins.
5. Add the tests above and update `docs/language-guide.md`,
   `docs/type-system.md`, and `specs/types.md`.

Each step must leave `cargo test --workspace`, `cargo clippy`, and CI green.