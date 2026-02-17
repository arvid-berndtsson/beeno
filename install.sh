#!/usr/bin/env sh
set -eu

REPO="${REPO:-arvid/beeno}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

resolve_version() {
  if [ -n "${VERSION:-}" ]; then
    echo "${VERSION#v}"
    return
  fi

  latest_url="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest")"
  tag="${latest_url##*/}"
  if [ -z "$tag" ]; then
    echo "error: failed to resolve latest release tag" >&2
    exit 1
  fi
  echo "${tag#v}"
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux) os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *)
      echo "error: unsupported OS: $os" >&2
      exit 1
      ;;
  esac

  case "$arch" in
    x86_64|amd64) arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  echo "${arch_part}-${os_part}"
}

verify_checksum() {
  archive="$1"
  sums_file="$2"

  if command -v sha256sum >/dev/null 2>&1; then
    grep " ${archive}$" "$sums_file" | sha256sum -c -
  elif command -v shasum >/dev/null 2>&1; then
    grep " ${archive}$" "$sums_file" | shasum -a 256 -c -
  else
    echo "error: need sha256sum or shasum to verify download" >&2
    exit 1
  fi
}

version="$(resolve_version)"
target="$(detect_target)"
archive="beeno-v${version}-${target}.tar.gz"
base_url="https://github.com/${REPO}/releases/download/v${version}"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT INT TERM

curl -fsSL "$base_url/$archive" -o "$tmpdir/$archive"
curl -fsSL "$base_url/SHA256SUMS.txt" -o "$tmpdir/SHA256SUMS.txt"

verify_checksum "$archive" "$tmpdir/SHA256SUMS.txt"

tar -xzf "$tmpdir/$archive" -C "$tmpdir"

mkdir -p "$INSTALL_DIR"
install "$tmpdir/beeno" "$INSTALL_DIR/beeno"

echo "Installed beeno ${version} to $INSTALL_DIR/beeno"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "note: add $INSTALL_DIR to PATH to run 'beeno' directly" ;;
esac
