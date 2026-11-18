#!/usr/bin/env bash
# NEX installer for Unix-like systems
# Usage: curl -fsSL https://raw.githubusercontent.com/cokovali10-sketch/NEX/master/install.sh | sh

set -e

REPO="https://github.com/cokovali10-sketch/NEX.git"
INSTALL_DIR="${NEX_HOME:-$HOME/.nex}"

echo "NEX installer"
echo "============="
echo "install dir: $INSTALL_DIR"

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust toolchain not found. Install from https://rustup.rs first."
    exit 1
fi

if ! command -v clang >/dev/null 2>&1; then
    echo "clang not found. Install LLVM/clang and add to PATH."
    exit 1
fi

if [ -d "$INSTALL_DIR/.git" ]; then
    echo "Updating existing installation..."
    git -C "$INSTALL_DIR" pull --ff-only
else
    echo "Cloning NEX..."
    git clone "$REPO" "$INSTALL_DIR"
fi

cd "$INSTALL_DIR"
cargo build --release

BIN_DIR="$INSTALL_DIR/target/release"
if [ ! -f "$BIN_DIR/nex" ]; then
    echo "Build failed: nex binary not found."
    exit 1
fi

# Add to PATH hint
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        echo "Add the following to your shell profile:"
        echo "  export PATH=\"$BIN_DIR:\$PATH\""
        ;;
esac

echo "NEX installed successfully."
echo "Run: nex --version"