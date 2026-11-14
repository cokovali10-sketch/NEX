# NEX

NEX is an experimental native compiled programming language and developer ecosystem.
The long-term goal is a readable, statically typed language with native performance,
a safe memory model, an LLVM-based compiler, a standard library, universal
interoperability, and a professional toolchain.

NEX 0.1 is intentionally small. It is a real vertical slice: `.nex` source is
lexed, parsed, type-checked, lowered to NEX IR, emitted as LLVM IR, and compiled
to a native executable with Clang. It is not an interpreter and not a wrapper
around another language runtime.

## Status

- Current milestone: **NEX 0.1 - Language Foundation / Native Compiler**.
- Implemented: lexer, parser, AST, semantic analysis, type inference for a small
  subset, NEX IR, LLVM textual IR backend, native executable generation, CLI,
  end-to-end tests.
- Implemented in the language: functions, bindings, `if`/`elif`/`else`,
  `while`, `for`, `const`, explicit casts, string equality, compound
  assignment, structs with scalar fields, and enums with `match`.
- Not implemented yet: generics, modules, imports, arrays, the full
  standard library, package manager, debugger, profiler, and NEX Studio.
- Platform support today: Windows x86-64 with Clang. The compiler is designed to
  target other platforms, but only the host platform is tested.

## Quick start

Build the toolchain:

```powershell
cargo build
```

Create and run a program:

```powershell
cargo run -p nex-cli -- new hello
cd hello
cargo run -p nex-cli -- run
```

Compile a single file directly:

```powershell
cargo run -p nex-driver --bin nexc -- examples/hello/src/main.nex --emit exe -o hello.exe
./hello.exe
```

Run tests:

```powershell
cargo test --workspace
```

Check the toolchain:

```powershell
cargo run -p nex-cli -- doctor
```

## Example

```nex
fn add(a: i64, b: i64) -> i64:
    return a + b

fn main():
    var total = 0
    var i = 1
    while i <= 5:
        total = total + i
        i = i + 1
    print(total)
    print(add(2, 3))
```

## Repository layout

```text
NEX/
  compiler/      Rust crates: diagnostics, lexer, parser, AST, semantic, IR, backend, driver
  runtime/       Small C runtime used by generated programs
  tools/cli/     The `nex` command-line interface
  tests/         Unit and end-to-end tests
  examples/      Example NEX projects
  docs/          User and developer documentation
  specs/         Language and compiler specifications
  studio/        Reserved for NEX Studio
```

## Documentation

- [Architecture](ARCHITECTURE.md)
- [Roadmap](ROADMAP.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)
- [Language specification](specs/language.md)
- [Grammar](specs/grammar.md)
- [Type system](specs/types.md)
- [Memory model](specs/memory-model.md)
- [Compiler architecture](specs/compiler.md)

## Design principles

- Write simple programs.
- Compile to native code.
- Keep the compiler honest: every supported feature has tests.
- Do not claim performance or compatibility without measurements.
- Do not hide language complexity behind unpredictable rules.
