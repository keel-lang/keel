#!/bin/sh
# Keel uninstaller
# Usage: curl -sSf https://keel-lang.dev/uninstall.sh | sh

set -e

INSTALL_DIR="${KEEL_INSTALL_DIR:-$HOME/.keel/bin}"

if [ -f "$INSTALL_DIR/keel" ]; then
  rm -f "$INSTALL_DIR/keel"
  rmdir "$INSTALL_DIR" 2>/dev/null || true
  rmdir "$(dirname "$INSTALL_DIR")" 2>/dev/null || true
  echo "Keel removed from $INSTALL_DIR"
  echo ""
  echo "You may also want to remove the PATH entry from your shell config:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
else
  echo "Keel not found at $INSTALL_DIR"
fi

# Clean up history
rm -f "$HOME/.keel_history"
