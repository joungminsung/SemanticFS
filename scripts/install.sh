#!/usr/bin/env bash
set -euo pipefail

REPO="anthropics/semanticfs"
BINARY="semfs"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

echo "SemanticFS Installer"
echo "===================="
echo ""

# -----------------------------------------------
# Try pre-built binary first, fall back to source
# -----------------------------------------------

case "${OS}" in
    linux)  TARGET_OS="unknown-linux-gnu" ;;
    darwin) TARGET_OS="apple-darwin" ;;
    *)      echo "Unsupported OS: ${OS}. Try building from source."; exit 1 ;;
esac

case "${ARCH}" in
    x86_64)        TARGET_ARCH="x86_64" ;;
    aarch64|arm64) TARGET_ARCH="aarch64" ;;
    *)             echo "Unsupported architecture: ${ARCH}. Try building from source."; exit 1 ;;
esac

TARGET="${TARGET_ARCH}-${TARGET_OS}"

# Try to fetch latest release
LATEST=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name"' | cut -d'"' -f4 || true)

if [ -n "${LATEST}" ]; then
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/${BINARY}-${TARGET}.tar.gz"
    INSTALL_DIR="${HOME}/.local/bin"
    mkdir -p "${INSTALL_DIR}"

    echo "Downloading SemanticFS ${LATEST} for ${TARGET}..."
    if curl -sfL "${DOWNLOAD_URL}" | tar xz -C "${INSTALL_DIR}" 2>/dev/null; then
        chmod +x "${INSTALL_DIR}/${BINARY}"
        echo ""
        echo "Installed ${BINARY} to ${INSTALL_DIR}/${BINARY}"

        # Add to PATH if not already there
        if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
            SHELL_RC="${HOME}/.$(basename "${SHELL}")rc"
            echo "export PATH=\"${INSTALL_DIR}:\$PATH\"" >> "${SHELL_RC}" 2>/dev/null || true
            echo "Added ${INSTALL_DIR} to PATH in ${SHELL_RC}"
            echo "Run: source ${SHELL_RC}"
        fi

        echo ""
        echo "Get started:"
        echo "  semfs index ~/Documents"
        echo "  semfs search \"React 프로젝트\""
        exit 0
    fi

    echo "Pre-built binary not available for ${TARGET}."
fi

# -----------------------------------------------
# Fall back: build from source
# -----------------------------------------------
echo "Building from source..."

# Install Rust if needed
if ! command -v cargo &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "${HOME}/.cargo/env"
fi

# Install FUSE dependencies
case "${OS}" in
    darwin)
        if command -v brew &> /dev/null; then
            if [ ! -d "/Library/Filesystems/macfuse.fs" ]; then
                echo "Installing macFUSE..."
                brew install --cask macfuse 2>/dev/null || echo "macFUSE requires manual install: https://osxfuse.github.io/"
            fi
        fi
        ;;
    linux)
        if command -v apt-get &> /dev/null; then
            sudo apt-get install -y -qq fuse3 libfuse3-dev pkg-config 2>/dev/null || true
        elif command -v dnf &> /dev/null; then
            sudo dnf install -y fuse3 fuse3-devel pkg-config 2>/dev/null || true
        elif command -v pacman &> /dev/null; then
            sudo pacman -S --noconfirm fuse3 pkg-config 2>/dev/null || true
        fi
        ;;
esac

# Clone and build
TMPDIR=$(mktemp -d)
echo "Cloning repository..."
git clone --depth 1 "https://github.com/${REPO}.git" "${TMPDIR}/semanticfs"
cd "${TMPDIR}/semanticfs"

echo "Building (this may take a few minutes)..."
cargo install --path crates/semfs-cli

# Cleanup
rm -rf "${TMPDIR}"

echo ""
echo "Installed! Run: semfs --help"
echo ""
echo "Get started:"
echo "  semfs index ~/Documents"
echo "  semfs search \"React 프로젝트\""
