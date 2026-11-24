# Installation

## Prerequisites
- Rust 1.100.0+ (for building the compiler)
- Clang 23.1.4+ (for native code generation)
- Windows x86_64 (primary platform)

## From Source
```bash
git clone https://github.com/cokovali10-sketch/NEX.git
cd NEX
cargo build --workspace
```

## Binaries
Download precompiled binaries from the GitHub releases page for your platform.

## Verify Installation
```bash
nex doctor
```

This checks the toolchain and prints:
- OS and architecture
- NEX version
- Clang availability and version
- End-to-end compile test

##PATH
Add the NEX binary directory to your PATH to use `nex` and `nexc` from anywhere.