#!/bin/bash
# peer-map.sh - map peer IDs (ed25519 pubkey hex) to validator seats
BASE=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony
OUT=/tmp/peer-map.txt
{
echo "=== ceremony-out layout ==="
find "$BASE/ceremony-out" -maxdepth 2 -type f 2>/dev/null | head -30
echo
echo "=== keys files ==="
for f in "$BASE"/ceremony-out/validator-*/keys.json "$BASE"/_secrets/keys.json "$BASE"/ceremony-out/keys.json; do
  [ -f "$f" ] && { echo "--- $f"; head -c 2000 "$f"; echo; }
done
echo
echo "=== node.toml peer/listen lines ==="
for f in "$BASE"/ceremony-out/validator-*/node-*.toml; do
  [ -f "$f" ] && { echo "--- $f"; grep -nE 'peer|listen|dial|seat|address' "$f" | head -15; }
done
echo
echo "=== search for the two resolver-fail peer ids ==="
grep -rl 'c98739580820987b\|62e51f5e242b142ae7' "$BASE" 2>/dev/null | head
} > "$OUT" 2>&1
cat "$OUT"

