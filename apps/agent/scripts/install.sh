#!/bin/sh
# Codux headless host (`codux`) installer / uninstaller.
#
# Install   download the prebuilt codux-agent binary for this machine and put it
#           on your PATH as `codux` (no build toolchain needed).
# Uninstall stop the host, remove its OS service, and delete the binary.
#
#   curl -fsSL https://raw.githubusercontent.com/duxweb/codux/main/apps/agent/scripts/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --uninstall
#
# Options (pass after `sh -s --`, or as env vars):
#   --beta              newest build incl. pre-releases             (CODUX_CHANNEL=beta)
#   --stable            newest stable build  [default]              (CODUX_CHANNEL=stable)
#   --version <x.y.z>   install an exact version                    (CODUX_VERSION=x.y.z)
#   --dir <path>        install / locate dir                        (CODUX_INSTALL_DIR=path)
#   --mirror <prefix>   prepend a download mirror (slow GitHub?)    (CODUX_DOWNLOAD_BASE=prefix)
#   --setup             run `codux config` + `codux install` after  (CODUX_SETUP=1)
#   --uninstall         remove the host (stops it, removes service + binary)
#   --purge             with --uninstall, also delete the data dir  (config, pairings, logs)
#   --help              show this help
set -eu

REPO="duxweb/codux"
BIN_NAME="codux"
MODE="install"
CHANNEL="${CODUX_CHANNEL:-stable}"
VERSION="${CODUX_VERSION:-}"
INSTALL_DIR="${CODUX_INSTALL_DIR:-}"
MIRROR="${CODUX_DOWNLOAD_BASE:-}"
RUN_SETUP="${CODUX_SETUP:-0}"
PURGE=0

say()  { printf '%s\n' "$*"; }
info() { printf '\033[36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33mwarning:\033[0m %s\n' "$*" >&2; }
err()  { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

show_help() {
  cat <<'EOF'
Codux headless host (codux) installer / uninstaller.

Install   download the prebuilt codux-agent binary and put it on PATH as `codux`.
Uninstall stop the host, remove its OS service, delete the binary.

  curl -fsSL https://raw.githubusercontent.com/duxweb/codux/main/apps/agent/scripts/install.sh | sh
  curl -fsSL .../install.sh | sh -s -- --uninstall

Options (pass after `sh -s --`, or as env vars):
  --beta              newest build incl. pre-releases             (CODUX_CHANNEL=beta)
  --stable            newest stable build  [default]              (CODUX_CHANNEL=stable)
  --version <x.y.z>   install an exact version                    (CODUX_VERSION=x.y.z)
  --dir <path>        install / locate dir                        (CODUX_INSTALL_DIR=path)
  --mirror <prefix>   prepend a download mirror (slow GitHub?)    (CODUX_DOWNLOAD_BASE=prefix)
  --setup             run `codux config` + `codux install` after  (CODUX_SETUP=1)
  --uninstall         remove the host (stops it, removes service + binary)
  --purge             with --uninstall, also delete the data dir  (config, pairings, logs)
  --help              show this help

Examples:
  curl -fsSL .../install.sh | sh -s -- --beta --setup
  curl -fsSL .../install.sh | sh -s -- --mirror https://ghproxy.net/
  curl -fsSL .../install.sh | sudo sh -s -- --uninstall --purge
EOF
}

http_get() {
  # Fetch a URL to stdout. $1 = url
  if have curl; then
    curl -fsSL "$1"
  elif have wget; then
    wget -qO- "$1"
  else
    err "need either 'curl' or 'wget' installed"
  fi
}

download() {
  # Fetch a URL to a file, routing through MIRROR if set. $1 = url, $2 = dest
  _url="$1"
  [ -n "$MIRROR" ] && _url="${MIRROR}${_url}"
  if have curl; then
    curl -fSL --retry 3 -o "$2" "$_url"
  elif have wget; then
    wget -O "$2" "$_url"
  else
    err "need either 'curl' or 'wget' installed"
  fi
}

detect_os() {
  case "$(uname -s)" in
    Darwin) printf 'macos' ;;
    Linux)  printf 'linux' ;;
    MINGW*|MSYS*|CYGWIN*)
      err "Windows isn't supported by this script. Download codux-agent-<ver>-windows-x86_64.exe from https://github.com/$REPO/releases and add it to your PATH as codux.exe" ;;
    *) err "unsupported OS: $(uname -s)" ;;
  esac
}

detect_arch() {
  # On an Apple-Silicon Mac running under Rosetta, uname -m lies (x86_64); fix it.
  if [ "$(uname -s)" = "Darwin" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    printf 'aarch64'; return
  fi
  case "$(uname -m)" in
    arm64|aarch64) printf 'aarch64' ;;
    x86_64|amd64)  printf 'x86_64' ;;
    *) err "unsupported architecture: $(uname -m)" ;;
  esac
}

resolve_beta_tag() {
  # Newest release including pre-releases (the list endpoint returns newest first).
  body="$(http_get "https://api.github.com/repos/$REPO/releases?per_page=10")" \
    || err "could not query the GitHub releases API"
  tag="$(printf '%s\n' "$body" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
  [ -n "$tag" ] || err "could not resolve the latest pre-release tag"
  printf '%s' "$tag"
}

choose_install_dir() {
  if [ -n "$INSTALL_DIR" ]; then printf '%s' "$INSTALL_DIR"; return; fi
  if [ "$(id -u)" = "0" ] || [ -w /usr/local/bin ]; then
    printf '/usr/local/bin'
  else
    printf '%s/.local/bin' "$HOME"
  fi
}

find_installed_binary() {
  # Locate an installed `codux` for uninstall. Prefers --dir, then PATH, then common dirs.
  if [ -n "$INSTALL_DIR" ] && [ -x "$INSTALL_DIR/$BIN_NAME" ]; then
    printf '%s' "$INSTALL_DIR/$BIN_NAME"; return 0
  fi
  if have "$BIN_NAME"; then command -v "$BIN_NAME"; return 0; fi
  for d in /usr/local/bin "$HOME/.local/bin" /usr/bin /opt/homebrew/bin; do
    if [ -x "$d/$BIN_NAME" ]; then printf '%s' "$d/$BIN_NAME"; return 0; fi
  done
  return 1
}

data_dir() { printf '%s' "${CODUX_AGENT_DATA_DIR:-$HOME/.codux-agent}"; }

do_install() {
  OS="$(detect_os)"
  ARCH="$(detect_arch)"
  ASSET="codux-${OS}-${ARCH}"

  if [ -n "$VERSION" ]; then
    TAG="v${VERSION#v}"
    URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
    LABEL="$TAG"
  elif [ "$CHANNEL" = "beta" ]; then
    TAG="$(resolve_beta_tag)"
    URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
    LABEL="$TAG (beta channel)"
  else
    URL="https://github.com/$REPO/releases/latest/download/$ASSET"
    LABEL="latest stable"
  fi

  INSTALL_DIR="$(choose_install_dir)"
  DEST="$INSTALL_DIR/$BIN_NAME"

  info "Installing codux host: $LABEL  [$OS/$ARCH]"
  info "From: ${MIRROR}${URL}"
  info "To:   $DEST"

  mkdir -p "$INSTALL_DIR" 2>/dev/null \
    || err "cannot create $INSTALL_DIR — re-run with sudo or pass --dir <writable path>"
  [ -w "$INSTALL_DIR" ] \
    || err "$INSTALL_DIR is not writable — re-run with sudo or pass --dir <writable path>"

  TMP="$(mktemp "${TMPDIR:-/tmp}/codux.XXXXXX")" || err "could not create a temp file"
  trap 'rm -f "$TMP"' EXIT INT TERM

  download "$URL" "$TMP" \
    || err "download failed. If GitHub is slow/blocked here, retry with --mirror <prefix> (e.g. a ghproxy-style mirror). Releases: https://github.com/$REPO/releases"
  chmod +x "$TMP"
  mv -f "$TMP" "$DEST"
  trap - EXIT INT TERM

  # curl/wget don't set the quarantine xattr, but strip it defensively on macOS.
  if [ "$OS" = "macos" ] && have xattr; then
    xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true
  fi

  if ! INSTALLED="$("$DEST" version 2>/dev/null)"; then
    warn "installed to $DEST but '$BIN_NAME version' didn't run — wrong arch, or a broken download?"
    INSTALLED=""
  fi
  info "Installed: ${INSTALLED:-$DEST}"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) warn "$INSTALL_DIR is not on your PATH. Add it, e.g.:
    echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile && export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
  esac

  if [ "$RUN_SETUP" = "1" ]; then
    if [ -t 0 ]; then
      info "Running setup…"
      "$DEST" config
      "$DEST" install
    else
      warn "--setup needs an interactive terminal; run '$BIN_NAME config && $BIN_NAME install' yourself."
    fi
  fi

  say ""
  say "Done. Next steps:"
  say "  $BIN_NAME config     # device name + relay network"
  say "  $BIN_NAME install    # run as a startup service"
  say "  $BIN_NAME qrcode     # show the pairing QR for your phone/desktop"
  say ""
  say "Update later with:  $BIN_NAME update"
  say "Remove later with:  curl -fsSL .../install.sh | sh -s -- --uninstall"
}

do_uninstall() {
  if bin="$(find_installed_binary)"; then
    info "Removing codux host: $bin"
    # Stop the running host and remove its OS service before deleting the binary.
    "$bin" stop </dev/null >/dev/null 2>&1 || true
    "$bin" uninstall </dev/null >/dev/null 2>&1 || true
    if rm -f "$bin" 2>/dev/null; then
      info "Removed $bin"
    else
      err "could not remove $bin — re-run with sudo, e.g.:
    curl -fsSL .../install.sh | sudo sh -s -- --uninstall"
    fi
  else
    warn "no '$BIN_NAME' binary found (looked in --dir, PATH, and common bin dirs)."
  fi

  dd="$(data_dir)"
  if [ "$PURGE" = "1" ]; then
    if [ -d "$dd" ]; then
      rm -rf "$dd" && info "Purged data dir: $dd  (config, pairings, logs)"
    else
      info "No data dir at $dd to purge"
    fi
  elif [ -d "$dd" ]; then
    say ""
    say "Kept your config + pairings in $dd"
    say "Delete those too by re-running with --purge."
  fi

  say ""
  say "Uninstalled."
}

# ---- parse args -------------------------------------------------------------
while [ $# -gt 0 ]; do
  case "$1" in
    --beta)        CHANNEL="beta" ;;
    --stable)      CHANNEL="stable" ;;
    --version)     VERSION="${2:?--version needs a value}"; shift ;;
    --version=*)   VERSION="${1#*=}" ;;
    --dir)         INSTALL_DIR="${2:?--dir needs a value}"; shift ;;
    --dir=*)       INSTALL_DIR="${1#*=}" ;;
    --mirror)      MIRROR="${2:?--mirror needs a value}"; shift ;;
    --mirror=*)    MIRROR="${1#*=}" ;;
    --setup)       RUN_SETUP="1" ;;
    --uninstall)   MODE="uninstall" ;;
    --purge)       PURGE="1" ;;
    -h|--help)     show_help; exit 0 ;;
    *)             err "unknown option: $1 (try --help)" ;;
  esac
  shift
done

if [ "$MODE" = "uninstall" ]; then
  do_uninstall
else
  do_install
fi
