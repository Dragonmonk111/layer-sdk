#!/usr/bin/env bash
# R9 window 2 — REMOVE: shrink the set 5 -> 4. Seat .13 (op-3) retires.
# Continuing operators reuse their share-request (same Ed25519+MAYO identity);
# only their BLS share changes. Fresh threshold polynomial for n=4 (3-of-4).
set -euo pipefail

GTK=/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys
R=/root/r9

# 1. Coordinator collects share-requests from CONTINUING operators only —
#    the retiring seat (.13) is simply absent from the deal.
mkdir -p "$R/share-requests-w2"
for oct in 10 11 12 14; do
  cp "$R/share-requests-w0/seat-$((oct-10)).json" "$R/share-requests-w2/"
done

# 2. Assemble n=4 over the four continuing share-requests -> fresh deal
"$GTK" assemble-genesis --input-dir "$R/share-requests-w2" \
  --output-dir "$R/ceremony-w2" --chain-id junoclaw-r9

# 3. Each continuing operator finalizes the share bound to ITS Ed25519 key.
for i in 0 1 2 3; do
  oct=$(python3 -c "import json; print([v['p2p_address'] for v in json.load(open('$R/ceremony-w2/shared.json'))['validators'] if v['validator_index']==$i][0].split(':')[0].split('.')[-1])")
  op=$((oct-10))
  "$GTK" finalize --keys "$R/op-$op/keys.json" \
    --bls-share "$R/ceremony-w2/validator-$i/bls-share.json" | tail -1
done

# 4. Stage into ip-<octet>/ dirs (compose mounts these)
bash /mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/r9-rehearsal/stage-window.sh w2
