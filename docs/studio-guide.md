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
- Project explorer showing `.nex` files from the selected folder.
- Syntax-highlighted editor, multiple tabs, and unsaved-change markers.
- Create a new `.nex` file using a relative path inside the project.
- Save all files with the toolbar or Ctrl+S.
- Build and run the project in the background with Ctrl+Enter.
- Build output is written to the project's `build/` folder.

## Current limits

- Enter the project folder path in the Open Project dialog; a native folder
  picker is not wired up yet.
- Project search currently searches the active file.
- NEX LSP diagnostics, completion, hover, and go-to-definition are not yet
  connected to the editor.
- Debugger and profiler panels are still planned.

The `tools/studio` local web editor and `tools/studio_native` Win32 prototype
remain separate experiments. The egui application is the desktop Studio
development path.
