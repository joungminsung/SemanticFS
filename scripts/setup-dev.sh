#!/usr/bin/env bash
set -euo pipefail

OS="$(uname -s)"
ARCH="$(uname -m)"

echo "============================================"
echo "  SemanticFS Development Environment Setup"
echo "============================================"
echo ""
echo "  OS:   ${OS} ${ARCH}"
echo ""

# -----------------------------------------------
# 1. Rust toolchain
# -----------------------------------------------
if ! command -v cargo &> /dev/null; then
    echo "[1/5] Rust not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "${HOME}/.cargo/env"
    echo "  Rust installed: $(rustc --version)"
else
    echo "[1/5] Rust found: $(rustc --version)"
fi

# Ensure minimum version (1.75)
RUST_VER=$(rustc --version | grep -oE '[0-9]+\.[0-9]+' | head -1)
RUST_MAJOR=$(echo "$RUST_VER" | cut -d. -f1)
RUST_MINOR=$(echo "$RUST_VER" | cut -d. -f2)
if [ "$RUST_MAJOR" -lt 1 ] || ([ "$RUST_MAJOR" -eq 1 ] && [ "$RUST_MINOR" -lt 75 ]); then
    echo "  Rust ${RUST_VER} is too old. Updating..."
    rustup update stable
fi

# -----------------------------------------------
# 2. Rust dev tools
# -----------------------------------------------
echo "[2/5] Installing Rust development tools..."
rustup component add rustfmt clippy
cargo install cargo-audit 2>/dev/null || true

# -----------------------------------------------
# 3. Platform-specific FUSE dependencies
# -----------------------------------------------
echo "[3/5] Checking platform dependencies..."

case "${OS}" in
    Darwin)
        # macOS: macFUSE
        if [ ! -d "/Library/Filesystems/macfuse.fs" ] && [ ! -f "/usr/local/lib/libfuse.dylib" ]; then
            echo "  macFUSE not found."
            if command -v brew &> /dev/null; then
                echo "  Installing macFUSE via Homebrew..."
                brew install --cask macfuse || {
                    echo ""
                    echo "  ⚠ macFUSE installation requires manual approval:"
                    echo "    1. Open System Settings → Privacy & Security"
                    echo "    2. Allow the macFUSE kernel extension"
                    echo "    3. Reboot"
                    echo "    4. Re-run this script"
                    echo ""
                    echo "  Without macFUSE, 'semfs search' (CLI mode) still works."
                }
            else
                echo "  Install macFUSE manually: https://osxfuse.github.io/"
                echo "  Or install Homebrew first: https://brew.sh/"
                echo "  Without macFUSE, 'semfs search' (CLI mode) still works."
            fi
        else
            echo "  macFUSE: installed"
        fi

        # Xcode Command Line Tools (needed for C compilation: SQLite, tree-sitter)
        if ! xcode-select -p &> /dev/null; then
            echo "  Installing Xcode Command Line Tools..."
            xcode-select --install 2>/dev/null || true
            echo "  Complete the Xcode CLT install dialog, then re-run this script."
            exit 0
        else
            echo "  Xcode CLT: installed"
        fi
        ;;

    Linux)
        # Linux: libfuse3
        if command -v apt-get &> /dev/null; then
            if ! dpkg -l libfuse3-dev &> /dev/null 2>&1; then
                echo "  Installing FUSE3 development libraries..."
                sudo apt-get update -qq && sudo apt-get install -y -qq fuse3 libfuse3-dev pkg-config build-essential
            else
                echo "  libfuse3-dev: installed"
            fi
        elif command -v dnf &> /dev/null; then
            if ! rpm -q fuse3-devel &> /dev/null 2>&1; then
                echo "  Installing FUSE3 development libraries..."
                sudo dnf install -y fuse3 fuse3-devel pkg-config gcc
            else
                echo "  fuse3-devel: installed"
            fi
        elif command -v pacman &> /dev/null; then
            if ! pacman -Q fuse3 &> /dev/null 2>&1; then
                echo "  Installing FUSE3..."
                sudo pacman -S --noconfirm fuse3 pkg-config base-devel
            else
                echo "  fuse3: installed"
            fi
        else
            echo "  Please install FUSE3 and pkg-config manually for your distro."
        fi
        ;;

    *)
        echo "  Windows or unknown OS. FUSE mount not supported."
        echo "  CLI mode ('semfs search') will still work."
        ;;
esac

# -----------------------------------------------
# 4. Build
# -----------------------------------------------
echo "[4/5] Building workspace..."
cargo build --workspace 2>&1 | tail -5

# -----------------------------------------------
# 5. Test
# -----------------------------------------------
echo "[5/5] Running tests..."
cargo test --workspace 2>&1 | tail -10

echo ""
echo "============================================"
echo "  Setup complete!"
echo "============================================"
echo ""
echo "  Next steps:"
echo "    cargo run --bin semfs -- index ~/Documents"
echo "    cargo run --bin semfs -- search \"React 프로젝트\""
echo ""
echo "  For semantic search (optional):"
echo "    brew install ollama && ollama serve &"
echo "    ollama pull multilingual-e5-base"
echo ""
