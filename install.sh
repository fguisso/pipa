#!/bin/sh
# pipa CLI installer. Detects your OS/arch, downloads the matching `pipa`
# binary from the latest GitHub release, and installs it to a bin dir on PATH.
#
#   curl -fsSL guisso.dev/pipa/install.sh | sh
#
# Env overrides:
#   PIPA_VERSION   git tag to install (default: latest)
#   PIPA_BIN_DIR   install dir (default: /usr/local/bin if root, else ~/.local/bin)
set -eu

REPO="fguisso/pipa"

err() { printf 'install: %s\n' "$1" >&2; exit 1; }
info() { printf '%s\n' "$1" >&2; }

need() { command -v "$1" >/dev/null 2>&1 || err "missing required tool: $1"; }
need uname

# --- pick a downloader -------------------------------------------------------
if command -v curl >/dev/null 2>&1; then
  dl() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  dl() { wget -qO "$2" "$1"; }
else
  err "need curl or wget"
fi

# --- detect target -----------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
ext=""
case "$os" in
  Linux)
    case "$arch" in
      x86_64|amd64) target="x86_64-unknown-linux-musl" ;;
      aarch64|arm64) target="aarch64-unknown-linux-musl" ;;
      *) err "unsupported linux arch: $arch" ;;
    esac ;;
  Darwin)
    case "$arch" in
      x86_64) target="x86_64-apple-darwin" ;;
      arm64) target="aarch64-apple-darwin" ;;
      *) err "unsupported macOS arch: $arch" ;;
    esac ;;
  MINGW*|MSYS*|CYGWIN*)
    target="x86_64-pc-windows-msvc"; ext=".exe" ;;
  *) err "unsupported OS: $os" ;;
esac

# --- resolve download URL ----------------------------------------------------
ver="${PIPA_VERSION:-latest}"
if [ "$ver" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/pipa-${target}${ext}"
else
  url="https://github.com/${REPO}/releases/download/${ver}/pipa-${target}${ext}"
fi

# --- choose bin dir ----------------------------------------------------------
if [ -n "${PIPA_BIN_DIR:-}" ]; then
  bindir="$PIPA_BIN_DIR"
elif [ "$(id -u)" = "0" ]; then
  bindir="/usr/local/bin"
else
  bindir="$HOME/.local/bin"
fi
mkdir -p "$bindir" || err "cannot create $bindir"

# --- download + install ------------------------------------------------------
tmp="$(mktemp)" || err "mktemp failed"
trap 'rm -f "$tmp"' EXIT INT TERM
info "downloading pipa ($target) ..."
dl "$url" "$tmp" || err "download failed: $url"
[ -s "$tmp" ] || err "downloaded file is empty: $url"

dest="$bindir/pipa${ext}"
chmod +x "$tmp"
mv "$tmp" "$dest" || err "cannot write $dest"
trap - EXIT INT TERM

info "installed: $dest"

# --- PATH hint ---------------------------------------------------------------
case ":$PATH:" in
  *":$bindir:"*) ;;
  *) info ""; info "note: $bindir is not on your PATH. Add it, e.g.:"
     info "  export PATH=\"$bindir:\$PATH\"" ;;
esac

info ""
info "next: pipa login --server <your-pipa-server-url>"
