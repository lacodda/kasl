#!/bin/sh
# kasl installer for macOS and Linux:
#   curl -fsSL https://raw.githubusercontent.com/lacodda/kasl/main/tools/install.sh | sh
set -eu

REPO="lacodda/kasl"

case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
    Darwin-arm64) TARGET="aarch64-apple-darwin" ;;
    *)
        echo "No prebuilt binary for $(uname -s)/$(uname -m); install with: cargo install kasl" >&2
        exit 1
        ;;
esac

# The tag comes from the /releases/latest redirect rather than the REST API:
# unauthenticated API calls are capped at 60 per hour per IP, and an installer
# that fails because someone else on the same address ran it is no installer.
# KASL_VERSION pins a specific release.
TAG="${KASL_VERSION:-}"
if [ -z "$TAG" ]; then
    LOCATION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" || true)
    TAG="${LOCATION##*/}"
fi
case "$TAG" in
    v[0-9]*) ;;
    *)
        echo "Cannot resolve the latest release of $REPO - set KASL_VERSION to a tag like v1.0.3" >&2
        exit 1
        ;;
esac

NAME="kasl-$TAG-$TARGET"
URL="https://github.com/$REPO/releases/download/$TAG/$NAME.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $URL"
curl -fsSL "$URL" | tar xz -C "$TMP"

BIN_DIR="${KASL_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"
# The archive may or may not carry a top-level directory; take the binary from
# wherever it landed rather than assuming a layout.
BIN=$(find "$TMP" -type f -name kasl -perm -u+x | head -n 1)
[ -n "$BIN" ] || { echo "The archive did not contain a kasl binary" >&2; exit 1; }
install -m 755 "$BIN" "$BIN_DIR/kasl"
echo "Installed kasl $TAG to $BIN_DIR/kasl"

# Short alias `ka`, unless something else in PATH already answers to that name
# (ours from a previous run does not count). KASL_NO_ALIAS=1 skips it.
# A symlink rather than the archive's own `ka` binary: it can never fall
# behind the version `kasl` was updated to.
if [ -z "${KASL_NO_ALIAS:-}" ]; then
    EXISTING=$(command -v ka 2>/dev/null || true)
    if [ -z "$EXISTING" ] || [ "$EXISTING" = "$BIN_DIR/ka" ]; then
        ln -sf kasl "$BIN_DIR/ka"
        echo "Alias ka -> kasl"
    else
        echo "Note: 'ka' already resolves to $EXISTING - alias skipped."
    fi
fi

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Note: add $BIN_DIR to your PATH." ;;
esac

# `init` is an interactive wizard, so it cannot run from here: piping this
# script into sh leaves stdin pointing at the script itself, not at a terminal.
echo "Next: run 'kasl setup' to set up monitoring, integrations and credentials."
