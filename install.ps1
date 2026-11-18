# NEX installer for Windows (PowerShell)
# Usage: iwr -useb https://raw.githubusercontent.com/cokovali10-sketch/NEX/master/install.ps1 | iex

$ErrorActionPreference = "Stop"

$repo = "https://github.com/cokovali10-sketch/NEX.git"
$installDir = if ($env:NEX_HOME) { $env:NEX_HOME } else { "$env:USERPROFILE\.nex" }

Write-Host "NEX installer"
Write-Host "============="
Write-Host "install dir: $installDir"

# Check Rust
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Rust toolchain not found. Install from https://rustup.rs first."
    exit 1
}

# Check clang
if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
    Write-Host "clang not found. Install LLVM/clang and add to PATH."
    exit 1
}

# Clone or update
if (Test-Path "$installDir\src") {
    Write-Host "Updating existing installation..."
    Push-Location $installDir
    git pull --ff-only
    Pop-Location
} else {
    Write-Host "Cloning NEX..."
    git clone $repo $installDir
}

# Build
Push-Location $installDir
cargo build --release
Pop-Location

$binDir = "$installDir\target\release"
if (-not (Test-Path "$binDir\nex.exe")) {
    Write-Host "Build failed: nex.exe not found."
    exit 1
}

# Add to PATH (user scope)
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$binDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$binDir", "User")
    Write-Host "Added $binDir to user PATH. Restart your shell."
}

Write-Host "NEX installed successfully."
Write-Host "Run: nex --version"