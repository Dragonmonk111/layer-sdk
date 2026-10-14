#!/usr/bin/env bash
# Sync the Windows repo's Rust workspace into the WSL copy and build slay3rd.
set -euo pipefail
SRC='/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain'
DST=/root/junoclaw-chain

for d in app packages contracts tools proto; do
  mkdir -p "$DST/$d"
  rsync -a --delete --exclude='target/' --exclude='node_modules/' "$SRC/$d/" "$DST/$d/"
  echo "synced $d"
done
cp "$SRC/Cargo.toml" "$SRC/Cargo.lock" "$DST/"
echo 'cargo files synced'

cd "$DST"
cargo build --release -p slay3rd 2>&1 | tail -6
