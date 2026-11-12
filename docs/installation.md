# Installation

## Requirements

- Rust stable (1.85 or newer).
- Clang (LLVM 15 or newer recommended).
- Git.

## Build

```powershell
git clone <repository>
cd NEX
cargo build
```

## Verify

```powershell
cargo test --workspace
cargo run -p nex-cli -- doctor
```
