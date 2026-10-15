#!/usr/bin/env bash
# Phase A step 2 — assemble the G2 genesis deal over collected share-requests.
# Run on the coordinator host (this machine, WSL or anywhere with the tool).
# Produces g2-window/ceremony/{shared.json, validator-<i>/{bls-share.json,node-<i>.toml}}
set -euo pipefail
cd "$(dirname "$0")"

GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
CHAIN_ID=${CHAIN_ID:-junoclaw-g1}   # same chain — this is a set change, not a new chain
SR=share-requests
OUT=ceremony

ls "$SR"/*.json >/dev/null || { echo "no share-requests in $SR/" >&2; exit 1; }
echo ">> share-requests:"; ls "$SR"

rm -rf "$OUT"
"$GTK" assemble-genesis --input-dir "$SR" --output-dir "$OUT" --chain-id "$CHAIN_ID"

echo ">> deal complete — validator set:"
python3 - <<'EOF'
import json
s = json.load(open("ceremony/shared.json"))
print("chain_id:", s["chain_id"], "| threshold:", s["threshold_required"], "of", s["threshold_total"])
for v in s["validators"]:
    print(f"  index={v['validator_index']} moniker={v['moniker']} p2p={v['p2p_address']}")
EOF
echo ">> next: distribute each validator-<i>/bls-share.json to its operator (encrypted channel)"
