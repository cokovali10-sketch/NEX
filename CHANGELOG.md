# Changelog

## Unreleased

### Added

- Fixed and growable arrays of struct and tuple values, including indexing,
  element assignment, and `for x in array`.
- `assert(condition, "message")` for assertion failures with a custom message.
- `pop(array)` removes and returns the last element of a growable array.
- `contains(array, value)` reports whether a value is present.
- `index_of(array, value)` returns the first matching index, or `-1`.
- `reverse(array)` reverses a fixed or growable array in place.
- `expr?` propagates `None`/`Err` from `Option`/`Result`-shaped structs.
- Generic type arguments in type annotations (`Box<i64>`).
- `defer:` blocks run at function exit, in reverse order, only when reached.
- `nex lsp` starts a language server over stdio (hover, go-to-definition,
  completion, document symbols, formatting, and live diagnostics).
- `nex check --json` prints diagnostics as JSON for tooling and AI agents.
- Built-in generic `Option<T>` and `Result<T, E>` structs (no declaration
  needed); `?` works on them out of the box.
- Generic function calls passing type parameters inside generic struct
  annotations (`fn wrap<T>(x: T) -> Option<T>`).
- Process and environment APIs: `env`, `args_count`, `arg_at`, `exit`.
  The generated `main` captures `argc`/`argv`; `nex run project -- args`
  forwards arguments to the program.
- Logging APIs `log_info`, `log_warn`, `log_error`: one UTC-timestamped line
  per call on stderr, serialized so worker threads cannot interleave lines.
- `docs/for-ai.md` and `AGENTS.md`: a self-contained reference for AI agents,
  including the `nex check` JSON format and an annotated ambitious example.
- `examples/kv_store`: a JSON HTTP key-value service (router, JSON DOM,
  mutex-guarded map, worker pool) verified end-to-end against a live server.
- Fixed synthetic span collisions between multiple `for x in array` loops.
- Comments and blank lines are legal as the first line of a block (they
  produced `NXP002` before).
- Monomorphized generic functions no longer share span-keyed type tables
  with each other or with the generic definition (e.g. two instantiations of
  the same `wrap<T>` no longer see each other's struct instantiations).
- Semantic analysis no longer panics when a generic struct was
  monomorphized from a definition outside the current program (built-in
  `Option`/`Result`).

### Changed

- `nex fmt` preserves `#` comments (re-attached before the statement they
  annotate, or kept on the same line) and module-level `var` declarations
  (which it used to drop).

## 1.0.1 (2026-11-19)

The first NEX release with downloadable binaries. The `v1.0.0` tag was cut
before CI was green and was never published with release assets; `v1.0.1`
supersedes it and includes all changes committed after that tag.

### Added

- `for x in array` iteration over fixed and growable arrays.
- Parallel assignment (`a, b = b, a`).
- `import "file.nex"` and `from "file.nex" import alias` syntax alongside
  `use alias from "file.nex"`.
- Bitwise operators for integers: `&`, `|`, `^`, `<<`, `>>`, `~`.
- Octal integer literals (`0o17`).
- String indexing (`s[1]`) and slicing (`s[1..3]`, `s[..3]`, `s[3..]`).
- Inherent methods via `impl Type:`.
- String interpolation (`f"value {expr}"`).
- Tuple destructuring in `let`/`var` bindings.
- Cross-platform target selection (`--target`) and `nex doctor --target`.
- A tag-triggered release workflow that tests, builds, and uploads Windows and
  Linux binaries with SHA-256 checksums.

### Fixed

- `nex lint` reports unused tuple-destructured bindings.
- Build metadata reports the real compiler version instead of `0.2.0`.
- Formatting and Clippy are clean across the workspace; CI is green on Windows
  and Ubuntu.
- `README`, `ROADMAP`, `CHANGELOG`, `docs/`, and `specs/` now describe the
  implemented feature set instead of the 0.1/0.2 subset.
- The repository URL in workspace metadata points at the actual GitHub repo.

## 1.0.0 (2026-11-18)

### Added

- String standard library: `str_contains`, `str_index_of`, `str_substring`,
  `str_upper`, `str_lower`, `str_trim`, with runtime implementations, semantic
  checks, and end-to-end tests.
- `examples/strings` demonstrating the string builtins.
- `docs/standard-library.md` documenting every implemented builtin.

- `nex fmt` formats NEX sources from the AST, with `--check` and `--write`
  modes. Compound assignments are preserved; comments are dropped (the lexer
  discards them) and the command warns before overwriting such a file.

- `nex lint` reports unused bindings, unused private functions,
  self-comparisons, constant conditions, and unused imports, with
  `file:line:column` output and a `--deny-warnings` flag for CI.

- Traits: `trait` declarations, `impl Trait for Type`, `self`, and
  `receiver.method(...)` calls dispatched statically to `Type.method`.

- Type aliases (`type Name = Type`).

- Extended `match`: literal patterns, guards (`case X if cond:`), and
  inclusive range patterns (`case 1..5:`).

- `unsafe:` blocks for grouping low-level operations.
- `nex add <path>` copies a local package into `deps/` and records it in `nex.toml`.

- `pass`, an explicitly empty statement, for empty function bodies, `else`
  branches, and `match` arms. The formatter emits it for empty blocks.

- `str_starts_with`, `str_ends_with`, `str_repeat`, and `str_replace`.

- `str_reverse` and `parse_i64`. `abs` now accepts `f64`, and `to_string`
  accepts `f64` and `bool`.

- `read_file`, `write_file`, `file_exists`, and `file_size`.

- Blocking TCP sockets: `tcp_listen`, `tcp_accept`, `tcp_connect`,
  `tcp_write`, `tcp_read`, `tcp_close` (Windows and POSIX).

- HTTP helpers: `http_method`, `http_path`, `http_version`, `http_header`,
  `http_body`, `http_response`. A NEX program can now serve HTTP/1.1.

- Design decision for growable arrays: `specs/dynamic-arrays.md` (representation,
  ABI, type-system migration, and the test plan).

- Time helpers: `unix_time`, `now_ms` (monotonic), `sleep_ms`, and `http_date`
  (RFC 7231 IMF-fixdate).

- `str_index_of_from`, `char_at`, and `char_code` for parsing and splitting.

- Growable arrays: `[T]`, `push`, `len`, bounds-checked indexing, and
  assignment to elements. See `specs/dynamic-arrays.md`.

- JSON parsing: `json_parse`, `json_type`, `json_len`, `json_at`, `json_get`,
  `json_str`, `json_num`, `json_bool`, `json_is_null`.

- `str_split_count` / `str_split_at` for splitting, and `json_stringify`.

- String-keyed hash maps: `map_new`, `map_set`, `map_get`, `map_has`, `map_remove`,
  `map_len`, `map_key_at`.

- Threads: `thread_spawn`, `thread_join`, `thread_running` (OS threads, no args yet).

- Mutexes: `mutex_new`, `mutex_lock`, `mutex_unlock`.

- Module-level `var` globals, shared by functions and threads.

- Router: `router_new`, `router_add`, `router_match`, `router_param` with `:param` capture and 404/405 results.

- `thread_spawn(name, arg)` passes a single `i64` argument to the worker.

- Thread pool: `pool_new`, `pool_submit`, `pool_workers` (bounded queue, fixed workers).

- String builder: `builder_new`, `builder_append`, `builder_len`, `builder_string`,
  `builder_clear` (amortised O(1) appends instead of quadratic `concat`).

- JSON output helpers: `json_escape` and `json_quote`, plus an example that builds JSON and parses it back.

- Math: `floor`, `ceil`, `round`, `trunc` for `f64`.

### Fixed

- The lexer now skips a leading UTF-8 BOM instead of reporting `unexpected character`.
- `use` declarations compile again after the `file` field was added to the AST node.
- The driver no longer imports `std::collections::HashSet` unused.


## 0.1.0 - 2026-11-12

### Added

- Nested modules (`module name:`) and `use` aliases.
- `pub` visibility for module items, enforced on access and `use`.
- Tuple literals and positional field access.
- Array literals and indexing reads/writes.
- Runtime bounds checking for array indexing.
- Array and tuple type annotations, parameters, and return values.
- Enum declarations and `match` over enum values.
- Struct declarations with scalar fields, construction, field access, and
  field assignment.
- Printing for struct, array, and tuple values.
- Struct, array, and tuple constants.
- Equality and inequality for struct, array, and tuple values.
- Nested struct fields, including reads, copies, and assignment.
- Struct parameters and return values by value.
- String equality and inequality (`==`, `!=`) with content comparison.
- String ordering (`<`, `<=`, `>`, `>=`) using lexicographic comparison.
- Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=`.
- Explicit `as` casts between numeric types, `bool`, and `char`.
- Top-level `const` declarations with compile-time constant initializers.
- `for ... in ... .. ...` loops.
- Definite-return checking for `while true` and constant `if` conditions.
- Initial NEX workspace and repository governance files.
- Indentation-aware lexer with diagnostics.
- Recursive-descent parser for functions, bindings, control flow, and expressions.
- AST and semantic analysis with type inference for a small subset.
- NEX IR and LLVM textual IR backend.
- Native executable generation through Clang.
- `nex` project CLI and `nexc` compiler driver.
- External C function declarations and calls.
- Reproducible build metadata in `build.json`.
- Debug symbols generated with Clang `-g`.
- Built-in `len` and `abs`.
- Built-in `min`, `max`, `sqrt`, and `pow`.
- Built-in `assert` and `panic`.
- Built-in `alloc` and `free`.
- Built-in `peek` and `poke`.
- Built-in `concat` and `to_string`.
- Constant folding for compile-time constant expressions.
- End-to-end tests for compilation and execution.
- Initial language, grammar, type, memory, compiler, and diagnostics specifications.
- Help suggestions in diagnostic output for common errors.

### Known limitations

- Only Windows x86-64 is tested.
- No generics, traits, or standard library yet.
- Struct fields must be scalar types; struct values cannot be printed or used
  as constants yet.
- Arrays and tuples cannot yet be used as parameter or return types.
- No ownership or lifetime checking yet.
- `pub` declarations are parsed but visibility is not enforced yet.
- `nex add`, `nex benchmark`, `nex debug`, and `nex profile` are not
  implemented yet.
- `nex fmt` drops comments, because the lexer discards them.
- `nex` arithmetic is limited to numeric types, `bool`, `char`, and `string`.
