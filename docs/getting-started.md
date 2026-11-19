# Getting Started

## Install

Download the archive for your platform from
[GitHub Releases](https://github.com/cokovali10-sketch/NEX/releases), extract
the `nex` binary, and add it to `PATH`. Compiling NEX programs requires Clang in
`PATH`.

You can also build from source:

```powershell
cargo build --release
# binary: target/release/nex.exe (Windows) or target/release/nex (Linux)
```

Verify the toolchain and a small end-to-end compile:

```powershell
nex doctor
```

## Create a project

```powershell
nex new hello
cd hello
```

Edit `src/main.nex`:

```nex
fn main():
    print("Hello from NEX!")
```

Run it:

```powershell
nex run
```

Build an optimized executable:

```powershell
nex build --release
```

Type-check without building:

```powershell
nex check
```

## Test, format, and lint

```powershell
nex test                    # compile and run every file in tests/
nex fmt --check src/main.nex
nex fmt --write src/main.nex
nex lint src/main.nex
```

## A larger example

```nex
fn length_squared(x: i64, y: i64) -> i64:
    return x * x + y * y

fn main():
    let points: [i64; 6] = [1, 2, 3, 4, 5, 6]
    var total = 0

    for value in points:
        total = total + value

    let xsquared = length_squared(points[0], points[1])
    print(f"total: {total}, first squared: {xsquared}")
```

`for value in points` iterates over the fixed array, and the f-string inserts
computed values into the printed string.

Arrays of structs and tuples are supported for fixed and growable arrays,
including `for` iteration and element assignment.

## Next steps

- [Language guide](language-guide.md)
- [Standard library](standard-library.md)
- [CLI reference](cli-reference.md)
- [Language specification](../specs/language.md)
- [Examples](../examples)
