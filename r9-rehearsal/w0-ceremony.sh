#!/usr/bin/env bash
# R9 rehearsal — window-0 baseline ceremony (4-seat set formed via Phase-A)
set -euo pipefail

GTK=/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys
R=/root/r9
OUT=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/r9-rehearsal

rm -rf "$R" && mkdir -p "$R/share-requests-w0" "$OUT/w0"

# Seat A,B,C,D — static docker IPs on the rehearsal subnet
for i in 0 1 2 3; do
  ip=$((i+10))
  "$GTK" keygen-share --output-dir "$R/op-$i" --name "seat-$i" --p2p "172.31.0.$ip:7001" >/dev/null
  cp "$R/op-$i/share-request.json" "$R/share-requests-w0/seat-$i.json"
done

# Coordinator assembly — n=4 N3f1
"$GTK" assemble-genesis --input-dir "$R/share-requests-w0" \
  --output-dir "$R/ceremony-w0" --chain-id junoclaw-r9

# Each validator finalizes its share into keys.json.
# validator_index is the SORTED Ed25519 position — map each share package
# back to its operator via the p2p_address in shared.json.
for i in 0 1 2 3; do
  ip=$(python3 -c "import json; print([v['p2p_address'] for v in json.load(open('$R/ceremony-w0/shared.json'))['validators'] if v['validator_index']==$i][0].split(':')[0].split('.')[-1])")
  op=$((ip-10))
  "$GTK" finalize --keys "$R/op-$op/keys.json" \
    --bls-share "$R/ceremony-w0/validator-$i/bls-share.json" | tail -1
done

# Stage keys + tomls for the Windows-side compose, named by validator_index
for i in 0 1 2 3; do
  mkdir -p "$OUT/w0/validator-$i"
  ip=$(python3 -c "import json; print([v['p2p_address'] for v in json.load(open('$R/ceremony-w0/shared.json'))['validators'] if v['validator_index']==$i][0].split(':')[0].split('.')[-1])")
  op=$((ip-10))
  cp "$R/op-$op/keys.json" "$OUT/w0/validator-$i/keys.json"
  cp "$R/ceremony-w0/validator-$i/node-$i.toml" "$OUT/w0/validator-$i/node.toml"
done
cp "$R/ceremony-w0/shared.json" "$OUT/w0/shared.json"
echo "=== w0 staged to $OUT/w0 ==="
