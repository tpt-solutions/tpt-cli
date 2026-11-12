#!/bin/sh
# Installs tptcli from a GitHub Release, after checking its SHA-256 checksum.
#
#   curl -fsSL https://raw.githubusercontent.com/TPT-Solutions/tpt-cli/master/install.sh | sh
#
# Environment variables:
#   TPTCLI_VERSION      tag to install, such as v0.1.0 (default: the latest release)
#   TPTCLI_INSTALL_DIR  where to put the binary (default: $HOME/.local/bin)

set -eu

REPO="TPT-Solutions/tpt-cli"
INSTALL_DIR="${TPTCLI_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
    echo "install.sh: $*" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

need curl
need tar
need uname

# Map this machine to a release target triple.
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
    Linux) os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *) fail "unsupported operating system: $os (see the README for Windows)" ;;
esac
case "$arch" in
    x86_64 | amd64) arch_part="x86_64" ;;
    arm64 | aarch64) arch_part="aarch64" ;;
    *) fail "unsupported architecture: $arch" ;;
esac
target="$arch_part-$os_part"

# Resolve the tag. The "latest" redirect avoids the GitHub API rate limit.
if [ -n "${TPTCLI_VERSION:-}" ]; then
    tag="$TPTCLI_VERSION"
else
    tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" | sed 's#.*/##')"
fi
case "$tag" in
    v*) ;;
    *) fail "could not determine the release tag (got '$tag')" ;;
esac
version="${tag#v}"
asset="tptcli-v$version-$target.tar.gz"
base="https://github.com/$REPO/releases/download/$tag"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading tptcli $tag for $target"
curl -fsSL -o "$tmp/$asset" "$base/$asset" || fail "download failed: $base/$asset"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" || fail "download failed: $base/SHA256SUMS"

# Verify the checksum before extracting anything.
expected="$(grep " $asset\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "$asset is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
else
    need shasum
    actual="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"
fi
[ "$expected" = "$actual" ] || fail "checksum mismatch for $asset (expected $expected, got $actual)"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/tptcli-v$version-$target/tptcli" "$INSTALL_DIR/tptcli"

echo "Installed $INSTALL_DIR/tptcli"
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) echo "Add $INSTALL_DIR to your PATH to run tptcli from anywhere." ;;
esac
"$INSTALL_DIR/tptcli" --version
