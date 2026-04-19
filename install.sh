#!/bin/sh
# Keel installer — https://keel-lang.dev
# Usage:
#   curl -sSf https://keel-lang.dev/install.sh | sh
#
# Respects: KEEL_INSTALL_DIR (default: $HOME/.keel/bin)

set -e

REPO="keel-lang/keel"
INSTALL_DIR="${KEEL_INSTALL_DIR:-$HOME/.keel/bin}"

# Detect OS and arch
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64) ARCH="aarch64" ;;
  arm64)   ARCH="aarch64" ;;
  *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

case "$OS" in
  darwin)
    if [ "$ARCH" != "aarch64" ]; then
      echo "Prebuilt macOS binaries are Apple Silicon only."
      echo "Intel Macs can build from source:"
      echo ""
      echo "  git clone https://github.com/$REPO.git"
      echo "  cd keel && cargo build --release"
      echo "  cp target/release/keel /usr/local/bin/"
      exit 1
    fi
    TARGET="${ARCH}-apple-darwin"
    ;;
  linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Get latest release tag
TAG=$(curl -sSf "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
if [ -z "$TAG" ]; then
  echo "Could not find latest release. Building from source..."
  echo ""
  echo "  git clone https://github.com/$REPO.git"
  echo "  cd keel && cargo build --release"
  echo "  cp target/release/keel /usr/local/bin/"
  exit 1
fi

URL="https://github.com/$REPO/releases/download/$TAG/keel-$TARGET.tar.gz"

echo "Installing Keel $TAG for $TARGET..."
echo ""

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download and extract
curl -sSfL "$URL" | tar xz -C "$INSTALL_DIR"

# Check if install dir is in PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    SHELL_NAME=$(basename "$SHELL")
    case "$SHELL_NAME" in
      zsh)  RC="$HOME/.zshrc" ;;
      bash) RC="$HOME/.bashrc" ;;
      fish) RC="$HOME/.config/fish/config.fish" ;;
      *)    RC="$HOME/.profile" ;;
    esac

    echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$RC"
    echo "Added $INSTALL_DIR to PATH in $RC"
    echo "Run: source $RC"
    echo ""
    ;;
esac

echo "Keel $TAG installed to $INSTALL_DIR/keel"
echo ""
echo "  keel --version"
echo "  keel init my-agent"
echo "  keel run my-agent/main.keel"
echo ""
echo "Docs: https://keel-lang.dev"
