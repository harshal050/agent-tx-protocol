#!/bin/sh
# AgentTx installer for macOS and Linux.
#
#   curl -fsSL https://agent-tx-protocol.vercel.app/install.sh | sh
#
# Downloads the latest release from GitHub, checks its SHA-256 checksum and
# installs `agenttx` into ~/.agenttx/bin (override with AGENTTX_INSTALL_DIR).
# Pin a version with AGENTTX_VERSION=v0.1.0.

set -eu

REPO="harshal050/agent-tx-protocol"
GUIDE="https://agent-tx-protocol.vercel.app/docs/connect-ai-agents"
INSTALL_DIR="${AGENTTX_INSTALL_DIR:-$HOME/.agenttx/bin}"
VERSION="${AGENTTX_VERSION:-latest}"

say() { printf '%s\n' "$*"; }
fail() {
  printf '\nagenttx install: %s\n' "$*" >&2
  exit 1
}
need() { command -v "$1" > /dev/null 2>&1 || fail "the '$1' command is required but was not found."; }

need curl
need tar
need uname

os="$(uname -s)"
arch="$(uname -m)"
case "$os-$arch" in
  Linux-x86_64 | Linux-amd64) target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64 | Linux-arm64) target="aarch64-unknown-linux-gnu" ;;
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  *) fail "there is no ready-made download for $os ($arch) yet. Build from source instead: $GUIDE#option-b-build-from-source" ;;
esac

if [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi
file="agenttx-$target.tar.gz"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "Downloading $file ($VERSION)..."
curl -fsSL "$base/$file" -o "$tmp/$file" ||
  fail "download failed. Check that a release exists at https://github.com/$REPO/releases — or build from source: $GUIDE#option-b-build-from-source"

if curl -fsSL "$base/$file.sha256" -o "$tmp/$file.sha256" 2> /dev/null; then
  expected="$(cut -d ' ' -f 1 "$tmp/$file.sha256")"
  if command -v sha256sum > /dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$file" | cut -d ' ' -f 1)"
  else
    actual="$(shasum -a 256 "$tmp/$file" | cut -d ' ' -f 1)"
  fi
  [ "$expected" = "$actual" ] || fail "checksum mismatch — the download may be corrupted. Nothing was installed."
  say "Checksum verified."
fi

tar xzf "$tmp/$file" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/agenttx-$target/agenttx" "$INSTALL_DIR/agenttx"
chmod +x "$INSTALL_DIR/agenttx"

say ""
say "✓ AgentTx installed: $INSTALL_DIR/agenttx"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    say ""
    say "One more step — let your terminal find agenttx. Run this, then open a new terminal window:"
    case "$(basename "${SHELL:-sh}")" in
      zsh) say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.zshrc" ;;
      bash) say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.bashrc" ;;
      fish) say "  fish_add_path $INSTALL_DIR" ;;
      *) say "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile" ;;
    esac
    ;;
esac

say ""
say "Next: run  agenttx connect  to connect Claude Code, Codex, Cursor and other AI apps."
say "Guide: $GUIDE"
