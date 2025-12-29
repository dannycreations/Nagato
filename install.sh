#!/bin/bash
set -e

# --- Configuration ---
OWNER="dannycreations"
REPO="nagato"
ASSET_NAME="nagato-linux-amd64"
BINARY_NAME="nagato"

# --- Arch Detection ---
ARCH=$(uname -m)
if [ "$ARCH" != "x86_64" ]; then
  echo "Error: Unsupported Architecture: $ARCH. Only x86_64 is supported for Linux."
  exit 1
fi

# --- Versioning ---
VERSION=${1:-latest}
if [ "$VERSION" == "latest" ]; then
  URL="https://api.github.com/repos/$OWNER/$REPO/releases/latest"
else
  URL="https://api.github.com/repos/$OWNER/$REPO/releases/tags/$VERSION"
fi

# --- Download ---
echo "Fetching $VERSION release info..."

RELEASE_INFO=$(curl -sL -A "nagato-installer" "$URL")
DOWNLOAD_URL=$(echo "$RELEASE_INFO" | grep -oP '"browser_download_url":\s*"\K[^"]+' | grep "$ASSET_NAME" | head -n 1)

if [ -z "$DOWNLOAD_URL" ]; then
  echo "Error: Could not find binary for Linux-amd64 in $VERSION release."
  exit 1
fi

INSTALL_DIR="$HOME/.nagato/bin"
mkdir -p "$INSTALL_DIR"

DEST_PATH="$INSTALL_DIR/$BINARY_NAME"

echo "Downloading $ASSET_NAME..."
curl -L -o "$DEST_PATH" "$DOWNLOAD_URL"
chmod +x "$DEST_PATH"

# --- PATH check ---
if [ -f "$HOME/.zshrc" ]; then
  PROFILE="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
  PROFILE="$HOME/.bashrc"
else
  PROFILE="$HOME/.profile"
fi

if ! grep -q "$INSTALL_DIR" "$PROFILE"; then
  echo "Adding $INSTALL_DIR to PATH..."
  echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$PROFILE"
  echo "Please restart your terminal or run 'source $PROFILE' to use '$BINARY_NAME'."
fi

echo "Nagato installed to $DEST_PATH"
