#!/usr/bin/env bash
# Phase B — per-seat apply after the reshare ceremony.
#
# Each seat's working dir (local or remote) must contain:
#   dkg-info.json            — from ceremony/
#   logs/                    — all dealer log-*.json (public)
#   priv-<moniker>/          — dealing-priv-*-for-<ME>.json (private, sent per-seat)
#   share-requests/          — all four request files (public)
#   keys/keys.json           — their keys (share gets overwritten by finalize)
#
# This script: dkg-finalize → finalize → new node.toml → park journal.
# Configure the seat map below, then run. External operators get the same
# commands via OPERATOR-PACK.md.
set -euo pipefail
cd "$(dirname "$0")"
source ./anchor.env
: "${ANCHOR_HEIGHT:?run read-tips.sh first}" "${ANCHOR_DIGEST:?run read-tips.sh first}"

GTK=${GTK:-/root/junoclaw-chain/tools/generate-testnet-keys/target/release/generate-testnet-keys}
OUT=ceremony

apply_local() { # $1=moniker $2=keys_dir $3=config_dir $4=data_dir $5=service
  local name=$1 keys=$2 conf=$3 data=$4 svc=$5
  echo ">> $name (local)"

  "$GTK" dkg-finalize \
    --info "$OUT/dkg-info.json" --keys "$keys/keys.json" \
    --priv-dir "$OUT/priv-$name" --logs-dir "$OUT/logs" \
    --requests share-requests --out "$OUT/bls-share-$name.json"
  "$GTK" finalize --keys "$keys/keys.json" \
    --bls-share "$OUT/bls-share-$name.json" | tail -1

  python3 mk-node-toml.py "$OUT/shared.json" "$name" \
    "$ANCHOR_HEIGHT" "$ANCHOR_DIGEST" "$conf/node.toml" > "$conf/node.toml.g2"

  mv "$data/consensus" "$data/consensus.g2-bak"
  mv "$conf/node.toml.g2" "$conf/node.toml"
  echo "   staged — service $svc restarts via start.sh"
}

apply_remote() { # $1=moniker $2=ssh-prefix (e.g. "ssh -i key ubuntu@host") $3=remote_workdir
  local name=$1 sshp=$2 rwd=$3
  echo ">> $name (remote $rwd)"
  # Push the public material + this seat's private dealing files.
  $sshp "mkdir -p $rwd/{logs,priv,share-requests}"
  scp ${sshp#ssh } "$OUT/dkg-info.json" "$rwd/dkg-info.json" 2>/dev/null || \
    $sshp "cat > $rwd/dkg-info.json" < "$OUT/dkg-info.json"
  $sshp "cat > $rwd/shared.json" < "$OUT/shared.json"
  for f in "$OUT"/logs/*.json; do
    $sshp "cat > $rwd/logs/$(basename "$f")" < "$f"
  done
  for f in "$OUT"/priv-"$name"/*.json; do
    $sshp "cat > $rwd/priv/$(basename "$f")" < "$f"
  done
  for f in share-requests/*.json; do
    $sshp "cat > $rwd/share-requests/$(basename "$f")" < "$f"
  done
  echo "   pushed — run remotely:"
  echo "   $GTK dkg-finalize --info dkg-info.json --keys keys/keys.json \\"
  echo "     --priv-dir priv --logs-dir logs --requests share-requests --out bls-share.json"
  echo "   $GTK finalize --keys keys/keys.json --bls-share bls-share.json"
  echo "   then write node.toml (mk-node-toml.py), mv data/consensus aside, restart"
}

echo "!! edit seat calls below per ceremony/shared.json ordering !!"

# === SEAT MAP — fill after observe.sh prints the new set ===
# apply_local  home-vb  /mnt/c/.../keys        /config       /data        g1-node
# apply_remote ovh-uk1 "ssh -i ~/.ssh/ovh_g1 ubuntu@51.195.201.74" /home/ubuntu/g2-window
# apply_remote vahana  "ssh ravi@57.129.103.229"                   ~/g2-window
# apply_remote anatoly "ssh anatoly@<hetzner-ip>"                  ~/g2-window

echo ">> done — verify each seat finalized, then ./start.sh (anchor seat FIRST)"
