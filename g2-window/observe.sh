#!/usr/bin/env bash
# Phase A.5 — coordinator: observe the reshare from PUBLIC material only.
# Collect every dealer's log-*.json into ceremony/logs/ first.
# Emits ceremony/shared.json (validator set + UNCHANGED group key) and
# ceremony/dkg-output-1.json (the --previous for the NEXT window).
set -euo pipefail
cd "$(dirname "$0")"

GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
OUT=ceremony

ls "$OUT"/logs/*.json >/dev/null 2>&1 || { echo "no dealer logs in $OUT/logs/" >&2; exit 1; }

"$GTK" dkg-observe \
  --info "$OUT/dkg-info.json" \
  --logs-dir "$OUT/logs" \
  --requests share-requests \
  --out-dir "$OUT"

python3 - <<'EOF'
import json
s = json.load(open("ceremony/shared.json"))
old = json.load(open("ceremony/dkg-output-0.json"))
assert s["threshold_public_key_hex"] == old["threshold_public_key_hex"], \
    "GROUP KEY CHANGED — this was supposed to be a reshare!"
print(">> group key preserved:", s["threshold_public_key_hex"][:32])
print(">> new set:")
for v in s["validators"]:
    print(f"   index={v['validator_index']} {v['moniker']} {v['p2p_address']}")
EOF

echo ">> next: every player runs dkg-finalize locally (see apply.sh)"
