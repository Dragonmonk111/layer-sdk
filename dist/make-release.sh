#!/bin/sh
# Cut a distribution bundle for a JunoClaw node release.
#
#   sh dist/make-release.sh <binary> <version>
#
# Produces dist/release/<version>/ with:
#   slay3rd-linux-amd64   the binary (renamed for the manifest)
#   MANIFEST.sha256       sha256 of every shipped file
#   install.sh            the installer operators run
#
# Upload the directory contents to the GitHub release for <version>.

set -eu

BIN="${1:?usage: make-release.sh <binary> <version>}"
VER="${2:?usage: make-release.sh <binary> <version>}"

out="dist/release/$VER"
mkdir -p "$out"

cp "$BIN" "$out/slay3rd-linux-amd64"
cp dist/install.sh "$out/install.sh"
chmod +x "$out/slay3rd-linux-amd64" "$out/install.sh"

( cd "$out" && sha256sum slay3rd-linux-amd64 install.sh > MANIFEST.sha256 )

echo ">> bundle at $out"
cat "$out/MANIFEST.sha256"
