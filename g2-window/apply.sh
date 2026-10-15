#!/usr/bin/env bash
# Phase B step 6 — per-seat apply: finalize keys, write new node.toml with
# restart_anchor, park the old consensus journal.
# Runs against seats we control (ovh, home-vb, local rehearsal seat).
# External operators get the equivalent commands in their pack.
set -euo pipefail
cd "$(dirname "$0")"
source ./anchor.env   # ANCHOR_HEIGHT, ANCHOR_DIGEST — must be filled
: "${ANCHOR_HEIGHT:?fill anchor.env first}" "${ANCHOR_DIGEST:?}"

GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
SSH="ssh -i /mnt/c/Users/Taj/.ssh/ovh_g1 -o ConnectTimeout=10"

# map: validator_index -> (keys_dir on its host). Determined AFTER deal.sh
# from ceremony/shared.json ordering — operators check their index there.

apply_seat() { # $1=name $2=validator_index $3=remote "ssh ..." or "local" $4=data_dir $5=config_dir $6=keys_dir $7=service
  local name=$1 vi=$2 where=$3 data=$4 conf=$5 keys=$6 svc=$7
  echo ">> $name (index $vi)"

  if [ "$where" = local ]; then
    "$GTK" finalize --keys "$keys/keys.json" \
      --bls-share "ceremony/validator-$vi/bls-share.json" | tail -1
    cp "ceremony/validator-$vi/node-$vi.toml" "$conf/node.toml.g2"
    python3 - "$conf/node.toml.g2" <<EOF
import sys, re
p = sys.argv[1]
t = open(p).read()
t += '\nrestart_anchor = { height = $ANCHOR_HEIGHT, digest = "$ANCHOR_DIGEST" }\n'
open(p, "w").write(t)
EOF
    mv "$data/consensus" "$data/consensus.g2-bak"
    mv "$conf/node.toml.g2" "$conf/node.toml"
  else
    # remote path — push bls-share + staged toml, operator applies locally
    echo "   remote seat — push ceremony/validator-$vi/ and have the operator run finalize + journal park + restart"
  fi
}

echo "!! edit the apply_seat calls below to match the dealt indexes first !!"
# apply_seat home-vb <i> local  /data            /config       /keys        g1-node
# apply_seat ovh-uk1 <i> remote 51.195.201.74    /config       /keys        junoclaw-g1
# apply_seat seat5   <i> local  /data-seat5      /config-seat5 /keys-seat5  g1-seat5

echo ">> done — now run start.sh (anchor seat FIRST)"
