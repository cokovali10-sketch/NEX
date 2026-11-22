# NEX Diagnostics

NEX diagnostics are rendered with file name, line, column, source line, and a
caret. Each diagnostic has a stable code and a severity.

## Example

```text
error[NXS006]: type mismatch: expected `i64`, found `string`
 --> src/main.nex:2:13
  |
2 |     let x: i64 = "hi"
  |             ^^^
```

## Codes

- `NXL001`-`NXL012`: lexer.
- `NXP001`-`NXP011`: parser.
- `NXS001`-`NXS047`: semantic analysis.
- `NXS050`: use of a moved value (ownership stage 1).
- `NXS051` / `NXS052`: invalid dereference / borrow target (references).
- `NXS053`: conflicting borrows in one statement (borrow checker).
- `NXC001`-`NXC002`: compiler and code generation.

## Rules

- The compiler reports as many independent errors as practical.
- Warnings do not fail compilation.
- Diagnostic messages should name the involved symbol or type.
- New diagnostics should be added to tests.
