#!/bin/sh

OS="$(uname)"
case "$OS" in
  Linux) OS='unknown-linux-musl';;
  Darwin) OS='apple-darwin';;
  CYGWIN*|MINGW32*|MSYS*|MINGW*) OS='pc-windows-msvc';;
  *) echo "Unsupported OS: $OS"; exit 1;;
esac

REPO_USER="lacodda"
REPO_NAME="lyrn"
LATEST_VERSION=$(curl -s https://api.github.com/repos/$REPO_USER/$REPO_NAME/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_VERSION" ]; then
    echo "Failed to fetch the latest version"
    exit 1
fi

RELEASE_URL="https://github.com/$REPO_USER/$REPO_NAME/releases/latest/download/lyrn-$LATEST_VERSION-x86_64-$OS.tar.gz"

if [ "$OS" = "linux" ] || [ "$OS" = "darwin" ]; then
  DESTINATION="$HOME/.local/bin"
else  # windows
  DESTINATION="$USERPROFILE/AppData/Local/Programs/$REPO_USER/$REPO_NAME"
fi

echo "Downloading and unpacking $RELEASE_URL to $DESTINATION..."
mkdir -p "$DESTINATION"
curl -L "$RELEASE_URL" | tar -xz -C "$DESTINATION" --strip-components=1

echo "🎉 Congratulations! $REPO_NAME $LATEST_VERSION was successfully installed."
