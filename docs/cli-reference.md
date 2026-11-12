# CLI Reference

## `nex`

- `nex new <name>`: create a project.
- `nex init [path]`: initialize an existing directory.
- `nex build [path]`: build a project.
- `nex run [path] [-- args]`: build and run.
- `nex check [path]`: type-check.
- `nex test [path]`: run `.nex` files in `tests/`.
- `nex clean [path]`: remove `build/`.
- `nex doctor [path]`: check toolchain and project.
- `nex version`: print version.

## `nexc`

Compile a single file:

```powershell
nexc file.nex --emit exe -o file.exe
nexc file.nex --emit llvm-ir
nexc file.nex --emit obj
nexc file.nex --emit check
```
