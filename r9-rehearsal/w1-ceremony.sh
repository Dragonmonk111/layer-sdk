#!/usr/bin/env bash
# R9 window 1 — ADD: grow the set 4 -> 5. Seat E (op-4, 172.31.0.14) joins.
# Continuing operators reuse their share-request (same Ed25519+MAYO identity);
# only their BLS share changes. Fresh threshold polynomial for n=5 (4-of-5).
set -euo pipefail

GTK=/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys
R=/root/r9

# 1. New operator generates identity + share-request (never leaves its machine)
"$GTK" keygen-share --output-dir "$R/op-4" --name "seat-4" --p2p "172.31.0.14:7001" >/dev/null
cp "$R/op-4/share-request.json" "$R/share-requests-w0/seat-4.json"

# 2. Coordinator: assemble n=5 over all five share-requests -> fresh deal
"$GTK" assemble-genesis --input-dir "$R/share-requests-w0" \
  --output-dir "$R/ceremony-w1" --chain-id junoclaw-r9

# 3. Each operator finalizes the share bound to ITS Ed25519 key.
#    Map ceremony validator_index -> p2p IP -> op dir.
for i in 0 1 2 3 4; do
  oct=$(python3 -c "import json; print([v['p2p_address'] for v in json.load(open('$R/ceremony-w1/shared.json'))['validators'] if v['validator_index']==$i][0].split(':')[0].split('.')[-1])")
  op=$((oct-10))
  "$GTK" finalize --keys "$R/op-$op/keys.json" \
    --bls-share "$R/ceremony-w1/validator-$i/bls-share.json" | tail -1
done

# 4. Stage into ip-<octet>/ dirs (compose mounts these)
bash /mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/r9-rehearsal/stage-window.sh w1
