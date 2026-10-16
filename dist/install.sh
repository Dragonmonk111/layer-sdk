#!/bin/sh
# JunoClaw slay3rd install — fetches the pinned release binary, verifies
# it against the bundle manifest, and installs it.
#
# Usage:
#   sh install.sh                    # installs to /usr/local/bin/slay3rd
#   DEST=/opt/junoclaw sh install.sh # custom prefix
#
# The manifest URL is the single source of truth: the binary is fetched
# from the same release and must sha256-match before it's installed.

set -eu

RELEASE="${JUNOCLAW_RELEASE:-v0.6.0-rc11}"
BASE="https://github.com/Dragonmonk111/layer-sdk/releases/download/${RELEASE}"
DEST="${DEST:-/usr/local/bin}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
need curl
need sha256sum

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo ">> fetching ${RELEASE} manifest"
curl -fsSL "$BASE/MANIFEST.sha256" -o "$work/MANIFEST.sha256"

echo ">> fetching slay3rd"
curl -fsSL "$BASE/slay3rd-linux-amd64" -o "$work/slay3rd"

# The manifest carries "<sha256>  slay3rd-linux-amd64" — verify against it.
want="$(awk '/slay3rd-linux-amd64/ {print $1}' "$work/MANIFEST.sha256")"
[ -n "$want" ] || { echo "manifest lacks slay3rd-linux-amd64 entry" >&2; exit 1; }
got="$(sha256sum "$work/slay3rd" | awk '{print $1}')"
[ "$got" = "$want" ] || { echo "CHECKSUM MISMATCH (want $want got $got) — aborting" >&2; exit 1; }
echo ">> checksum verified: $got"

chmod +x "$work/slay3rd"
"$work/slay3rd" --version >/dev/null 2>&1 || echo "!! binary runs but --version failed (glibc too old?) — continuing anyway"

mkdir -p "$DEST" 2>/dev/null || sudo mkdir -p "$DEST"
cp "$work/slay3rd" "$DEST/slay3rd" 2>/dev/null || sudo cp "$work/slay3rd" "$DEST/slay3rd"
echo ">> installed: $DEST/slay3rd"
echo "   next: place node-<i>.toml + keys.json from your bundle, then run:"
echo "   $DEST/slay3rd /path/to/node.toml"
