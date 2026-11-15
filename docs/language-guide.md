# Language Guide (0.2)

See `specs/language.md` and `specs/grammar.md` for the authoritative 0.2 subset.

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
