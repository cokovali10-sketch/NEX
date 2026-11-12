# Contributing to NEX

NEX is developed as a long-term engineering project. Contributions should keep
the compiler honest: every supported feature must have tests and documentation.

## Development

```powershell
cargo fmt
cargo clippy --workspace --all-targets
cargo test --workspace
```

## Rules

- Do not add dependencies without justification.
- Do not weaken tests to make a change pass.
- Do not document unimplemented features as implemented.
- Keep diagnostics clear and stable.
- Update `CHANGELOG.md` for user-visible changes.
- Add or update tests for every behavior change.

## Commit style

Use concise, scoped commit messages:

```text
feat(lexer): add hexadecimal integer literals
fix(ir): correct comparison operand type
docs(specs): describe memory model
```
