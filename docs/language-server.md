# NEX Language Server

NEX ships a language server in the `nex` CLI. It speaks LSP (JSON-RPC 2.0)
over stdio using the standard `Content-Length` framing. The server is backed
by the same compiler frontend that `nex build` uses, so diagnostics, symbols,
and hover information match exactly what the build produces.

## Starting the server

```powershell
nex lsp
```

The server reads requests on stdin and writes responses on stdout. It is meant
to be driven by an editor, not typed by hand. Point your editor's NEX plugin at
the `nex lsp` command, with the workspace root set to the folder that contains
`nex.toml` or `src/main.nex`.

## Supported capabilities

| Capability | Notes |
|---|---|
| `initialize` | Responds with `textDocumentSync: 1` (full-document sync). |
| `textDocument/didOpen` / `didChange` / `didSave` | Updates the in-memory copy of the document and republishes diagnostics. Full and incremental (`range`) edits are applied. |
| `textDocument/publishDiagnostics` | Emitted on open, change and save. Each diagnostic carries `range`, `severity`, `code`, `source: "nex"` and `message`. |
| `textDocument/hover` | Markdown hover for keywords, built-in functions, and names declared in the current file (`fn`, `struct`, `enum`, `trait`, `const`, module-level `var`, `type`, and `impl` methods). |
| `textDocument/definition` | Jumps to the declaration inside the current file. Returns an empty array for built-ins and keywords. |
| `textDocument/completion` | Completes keywords, built-ins and symbols declared in the current file, filtered by the prefix being typed. |
| `textDocument/documentSymbol` | Outline of the current file: structs, enums, traits, `const`, `var`, type aliases, functions and `impl` blocks (with methods as children). |
| `textDocument/formatting` | Formats the whole document with `nex fmt`, replacing the full range with the canonical layout. |
| `shutdown` / `exit` | Clean shutdown; `exit` terminates the process. |

## Minimal client test

Any editor with an LSP client can connect to it. For example, with
[Neovim](https://neovim.io)'s built-in client:

```lua
vim.lsp.start({
  name = 'nex',
  cmd = { 'nex', 'lsp' },
  root_dir = vim.fs.dirname(
    vim.fs.find({ 'nex.toml', 'src/main.nex' }, { upward = true })[1]
  ),
})
```

When debugging, run `nex lsp` manually and feed it LSP frames. The test suite
goes through the full handshake, diagnostics, hover, definition, completion,
document symbols, formatting and shutdown; refer to
`tools/cli/tests/lsp.rs` for a complete example of driving the server.

## Diagnostics

On every open, change and save the server recompiles the active file with
`Emit::Check` and publishes the resulting diagnostics. Because the compiler
does full semantic analysis (not just a parse), LSP diagnostics match
`nex check` exactly. This includes:

- unknown names and type errors (`NXS***`),
- unreachable statements (`NXS017`),
- shape and `?`-operator mistakes
  (e.g. `NXS004` when `?` is used without an `Option`/`Result` return type),
- array and string indexing mistakes (`NXS036`).

## Current limitations

- Hover text describes declarations in the open file; local-variable
  documentation is not derived from the checker's scope table yet.
- Diagnostics use a `(line, character)` position in UTF-16 code units. Files
  with non-ASCII identifiers report diagnostics on the exact byte position;
  editors that do not use UTF-16 still display them correctly for ASCII
  sources.
- The server operates on the open file plus whatever the compiler resolves
  for it (imports included). There is no project-wide index, so renaming a
  public symbol in a large project does not update call sites automatically.
- Formatting replaces the whole document. `nex fmt` preserves `#` comments;
  they are re-attached before the statement they annotate.
- Incremental compilation is not implemented yet; each keystroke recompiles
  the open file. For small to medium NEX projects this is fast enough.
