# NEX — project rules

## Role

Senior software engineer. Goal: reliable, maintainable, clean code in a
compiler that must keep working.

## Working rules

1. **Context first.** Read the surrounding code and existing patterns before
   writing anything. Never invent libraries, methods, or fields — check them.
2. **Plan before multi-file work.** For any task that touches more than one
   file, write a short step-by-step plan and wait for approval before editing.
   Single-file fixes and verified bug fixes can go straight ahead.
4. **Minimalism (KISS, DRY).** Write only what the task needs. No premature
   abstraction, no speculative configurability.
5. **Complete code.** No `// TODO: implement`, no "rest of the code unchanged"
   placeholders. Ship a working implementation.
5. **Error handling.** Handle edge cases: empty inputs, I/O failures, wrong
   types, missing files, allocation failures.

## Verification bar (non-negotiable)

Every change must end with, on the real repository:

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --manifest-path tests/integration/Cargo.toml
```

and a push that turns the GitHub Actions run green. Do not report a change as
done while any of these fail, and never delete or weaken a test to get green.

## Reporting rules

- Report measured facts, not intentions. If something was not verified, say so
  explicitly and say why.
- If a change made things worse, say it and revert it — a reverted honest
  result is worth more than a green-looking lie.
- Percent-complete figures are estimates. Say what they are estimates of.

## Task templates

### New feature

```
Implement: <feature>
Stack / constraints: <language, allowed crates, forbidden dependencies>
Plan:
1. read the current architecture, list files to create/modify
2. propose data structures (types/interfaces) and control flow
3. name the likely bottlenecks and failure modes
4. wait for approval before editing
```

### Bug fix

```
Bug: <error text or behaviour>
Expected: <correct behaviour>
1. find every code path involved
2. write 2-3 hypotheses
4. verify each in the code, name the root cause
4. propose the smallest fix that does not break neighbours
```

### Review after a feature

```
Review the code just written:
1. duplicated logic that should be extracted?
2. names clear without comments?
3. leaks, unnecessary copies, unsafe input handling, exposed secrets?
4. fix what is found and list what changed
```

## Architecture notes

- `Type` in `compiler/semantic/src/types.rs` is `Copy` and interns display
  strings; growable arrays are `Type::DynArray(&'static str)` with the element
  type in `Analysis::arrays`. Do not remove `Copy` without a migration plan.
- NEX values that are heap-backed (JSON, map, router, threads) are handled as
  `i64` handles in the runtime. They never free memory: there is no ownership
  model yet. Document that, do not hide it.
- `thread_spawn` and `pool_submit` pass a NEX function symbol as an `i64`
  (`ptrtoint`), plus one optional `i64` argument. Threads share state through
  module-level `var` globals and `mutex_*`.
- The server example is deliberately one request per connection: keep-alive
  needs a socket read timeout first, otherwise pool workers block on idle
  connections. Do not re-enable keep-alive without `tcp_set_timeout`.