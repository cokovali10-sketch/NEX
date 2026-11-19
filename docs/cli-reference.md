# CLI Reference

## `nex`

- `nex new <name>`: create a project.
- `nex init [path]`: initialize an existing directory.
- `nex build [path]`: build a project (`--release`, `--max`, `--debug`,
  `--target <triple>`, `--emit exe|obj|llvm-ir|check`).
- `nex run [path] [-- args]`: build and run.
- `nex check [path]`: type-check without producing an executable. `nex check
  --json [file.nex|project]` prints diagnostics as JSON for tooling and AI
  agents; `nex check` without `--json` prints the human-readable diagnostic
  report. A bare `.nex` file can be checked in isolation, outside any project.
- `nex test [path]`: compile and run every `.nex` file in `tests/`.
- `nex clean [path]`: remove `build/`.
- `nex doctor [path] [--target <triple>]`: check the toolchain, target, and a
  small end-to-end compile.
- `nex version`: print the compiler version.
- `nex fmt [--check | --write] <file.nex>...`: format source files.
- `nex lint [--deny-warnings] <file.nex>...`: report suspicious code.
- `nex add <path>`: copy a local package into `deps/` and record it in
  `nex.toml`.
- `nex benchmark [path] [-n N]`: build with `--release` and time `N` runs.
- `nex debug [path]`: build with debug symbols and run the executable. This is
  a convenience wrapper; it does not attach a debugger.
- `nex profile [path]`: build with `--release`, run once, and print wall-clock
  time. This is not a sampling profiler.
- `nex lsp`: start a language server over stdio (`initialize`,
  `textDocument/didOpen|didChange|didSave`, `textDocument/hover`,
  `textDocument/definition`, `textDocument/completion`,
  `textDocument/documentSymbol`, `textDocument/formatting`,
  `textDocument/publishDiagnostics`). See `docs/language-server.md`.

### `nex fmt`

Formats a file by parsing it and printing the canonical layout, so the output is
always valid NEX.

```powershell
nex fmt src/main.nex            # print the formatted source
nex fmt --write src/main.nex    # rewrite the file in place
nex fmt --check src/main.nex    # exit 1 when the file is not formatted
```

Behaviour worth knowing:

- Indentation is always four spaces, and one blank line separates top-level
  items. Extra blank lines inside a block are removed.
- Compound assignments are preserved: `x += 1` stays `x += 1`.
- `x = x + 1` is normalized to `x += 1`, because the parser records both forms
  as the same statement.
- **Comments are dropped.** NEX comments (`#`) are discarded by the lexer, so
  formatting a commented file loses them. `nex fmt` prints a warning to stderr
  before overwriting such a file, and refuses to touch a file that does not
  parse.

### `nex lint`

Reports suspicious but legal code. The linter runs on the parsed AST, so it
stays silent on files that do not compile.

```powershell
nex lint src/main.nex
nex lint --deny-warnings src/main.nex   # exit 1 when a warning is reported
```

Checks in NEX 1.0:

| Code | Check |
|---|---|
| `NXL001` | a local binding is declared but never used (including tuple-destructured names) |
| `NXL002` | a private function is never called |
| `NXL003` | an expression is compared with itself |
| `NXL004` | an `if` or `while` condition is a constant |
| `NXL005` | a `use` item is never referenced |

Bindings starting with `_` are exempt from `NXL001`.

## `nexc`

Compile a single file:

```powershell
nexc file.nex --emit exe -o file.exe
nexc file.nex --emit llvm-ir
nexc file.nex --emit obj
nexc file.nex --emit check
nexc file.nex --target x86_64-w64-windows-gnu
nexc --version
```
