# Language Guide (1.0)

See `specs/language.md` and `specs/grammar.md` for the authoritative 1.0 language
surface.

## Functions

```nex
fn add(a: i64, b: i64) -> i64:
    return a + b
```

## Bindings

```nex
let x = 10
var y = 20
y = y + x
```

## Control flow

```nex
if x < 0:
    print("negative")
elif x == 0:
    print("zero")
else:
    print("positive")

while x > 0:
    x = x - 1
```

## Printing

```nex
print("value: ")
println(42)
```

## Empty blocks

`pass` is a statement that does nothing. It exists so a block can be empty:

```nex
fn not_implemented():
    pass

fn main():
    match status:
        case Ok:
            pass
        case Error:
            print("failed")
```

The formatter (`nex fmt`) emits `pass` for an empty block, so formatting and
parsing stay compatible.

## Growable arrays

`[T]` is a growable array. It starts empty, grows with `push`, and is indexed
like any other array. Every access is bounds-checked.

```nex
fn main():
    var names: [string] = []
    push(names, "ann")
    push(names, "bob")
    println(len(names))   // 2
    println(names[0])     // ann
    names[1] = "bobby"
```

`[T; N]` (fixed size) is a different type and keeps its length in the type.
Assigning one name to another copies the header, so both names see the same
elements; see `docs/memory-model.md`.

## Error propagation

`?` works on `Option<T>` and `Result<T, E>`. NEX has both as built-in
generic structs (`Option` has `has: bool`, `value: T`; `Result` has
`ok: bool`, `value: T`, `error: E`), so no declaration is required:

```nex
fn find(xs: [i64], target: i64) -> Option<i64>:
    for x in xs:
        if x == target:
            return Option(true, x)
    return Option(false, 0)

fn use_option(target: i64) -> Option<i64>:
    let xs: [i64] = [1, 2, 3]
    let found = find(xs, target)?
    return Option(true, found + 10)
```

`expr?` returns the inner value on success and returns early from the
function on `None`/`Err`. The function must return the same Option/Result
shape. User-declared structs with the same field names (`has`/`value`,
`ok`/`value`/`error`) work too — `?` is convention-based.


A `var` at the top level is a module-level binding. Globals are shared by every
function and every thread, which is what makes shared state possible:

```nex
var counter = 0
var lock = 0

fn bump():
    mutex_lock(lock)
    counter = counter + 1
    mutex_unlock(lock)

fn main():
    lock = mutex_new()
    var a = thread_spawn("bump")
    var b = thread_spawn("bump")
    thread_join(a)
    thread_join(b)
    println(counter)   // 2000
```

Globals have no type annotation requirement: the type is inferred from the
initialiser. They must be declared with `var`, never `let`.
