#!/usr/bin/env bash
# Phase A — coordinator side of the G2 reshare ceremony.
#
# Unlike G1 (assemble-genesis = coordinator-dealt), G2 uses the dealer-free
# DKG reshare so the group public key — and every downstream trust root
# (Osmosis 08-wasm client, contract quorum checks) — is preserved.
#
# This script does the coordinator's half:
#   1. synthesize the round-0 Output from G1 genesis material (once)
#   2. collect share-requests -> dkg-info for round 1
# It then prints the exact commands each participant runs.
#
# Layout on the coordinator:
#   share-requests/     <moniker>.json for EVERY G2 validator (4 = old 3 + new)
#   dealers/            share-request.json of the DEALING seats (subset of the
#                       G1 set that still holds shares: ovh, home-vb, vahana)
#   g1-keys/            any G1 keys.json (for output synthesis + verify)
#   ceremony/           outputs: dkg-output-0.json, dkg-info.json, shared.json
set -euo pipefail
cd "$(dirname "$0")"

GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
CHAIN_ID=${CHAIN_ID:-junoclaw-g1}
ROUND=${ROUND:-1}
G1_KEYS=${G1_KEYS:-g1-keys/home-vb.json}          # any finalized G1 keys.json
G1_SHARED=${G1_SHARED:-/g1-ceremony/ceremony-out/shared.json}
OUT=ceremony

mkdir -p "$OUT"

# Step 1 — synthesize the round-0 Output (idempotent; skips if present)
if [ ! -f "$OUT/dkg-output-0.json" ]; then
  "$GTK" dkg-synthesize-genesis-output \
    --keys "$G1_KEYS" --shared "$G1_SHARED" \
    --verify-keys g1-keys \
    --out "$OUT/dkg-output-0.json"
fi

# Step 2 — round-1 info: dealers = surviving G1 seats, players = new set
ls share-requests/*.json >/dev/null || { echo "no share-requests/" >&2; exit 1; }
ls dealers/*.json        >/dev/null || { echo "no dealers/" >&2; exit 1; }

"$GTK" dkg-info \
  --requests share-requests \
  --dealers dealers \
  --chain-id "$CHAIN_ID" \
  --round "$ROUND" \
  --previous "$OUT/dkg-output-0.json" \
  --out "$OUT/dkg-info.json"

cat <<'EOF'

=== NEXT: distribute ceremony/dkg-info.json to every participant ===

Each DEALER (ovh, home-vb, vahana — a seat holding a G1 share) runs:
  generate-testnet-keys dkg-deal --info dkg-info.json --keys keys.json --out-dir dkg-out
  -> sends coordinator: dealing-pub-<me>.json AND dealing-priv-<me>-for-<each>.json
  (priv files are per-recipient; send each ONLY to that player)

Each PLAYER (all 4 seats incl. anatoly) then runs:
  generate-testnet-keys dkg-ack --info dkg-info.json --keys keys.json \
    --priv-dir <dir of dealing-priv-*-for-ME.json> --out-dir acks
  -> sends coordinator: all ack-*.json files

Each DEALER then runs:
  generate-testnet-keys dkg-log --info dkg-info.json --keys keys.json \
    --seed-file dkg-out/dealer.seed --acks-dir <all acks for me> --out-dir logs
  -> sends coordinator: log-<me>.json ; then deletes dealer.seed

Coordinator collects all logs into ceremony/logs/ then runs observe.sh.
Each player runs dkg-finalize to produce their own bls-share.json — no
secret share ever crosses the wire (only encrypted dealing messages).
EOF
