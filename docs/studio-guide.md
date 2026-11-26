# NEX Studio

NEX Studio is a standalone desktop IDE built with Rust and egui. It opens real
NEX project folders and does not run in a browser.

## Run the desktop app

From the repository root in PowerShell:

```powershell
$env:CARGO_HOME = 'D:\NEXproject\cargo-home'
$env:CARGO_TARGET_DIR = 'D:\NEXproject\NEX\target'
cargo run --manifest-path tools/studio_egui/Cargo.toml -- 'D:\path\to\your\project'
```

Cargo downloads and build artifacts go to D:. To launch the already-built app,
run `target\debug\nex-studio-egui.exe` and choose **Open project folder**. You
can also pass a project folder as the first argument to the executable.

The folder should contain `nex.toml` or NEX source files. The build entry is
read from `[build] entry` in `nex.toml`; when that is missing, Studio looks for
`src/main.nex`, then `main.nex`.

## Current features

- Native desktop window with a dark NEX theme.
- Windows folder picker, editable path fallback, and up to ten recent projects.
- Nested project tree with `.nex` files and folders from the selected folder.
- Syntax-highlighted editor, multiple tabs, and unsaved-change markers.
- Create `.nex` files and folders using relative paths inside the project.
- Save all files with the toolbar or Ctrl+S.
- Search loaded NEX source files in the project and jump to matching lines.
- Debounced background compiler diagnostics in a clickable Problems panel.
- Prompts before closing unsaved tabs or exiting with unsaved changes.
- Build and run the project in the background with Ctrl+Enter.
- Build output is written to the project's `build/` folder.

## Current limits

- The native folder picker is available on Windows; other platforms can use the
  editable path field.
- Background diagnostics use the NEX compiler directly. The editor does not yet
  use the `nex lsp` protocol for completion, hover, or go-to-definition.
- Debugger and profiler panels are still planned.

The `tools/studio` local web editor and `tools/studio_native` Win32 prototype
remain separate experiments. The egui application is the desktop Studio
development path.
