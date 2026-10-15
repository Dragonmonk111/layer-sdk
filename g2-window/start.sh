#!/usr/bin/env bash
# Phase B step 7 — restart the set. The seat holding the anchor tip MUST
# start first so lagging seats can fetch the gap from it.
set -euo pipefail
cd "$(dirname "$0")"
source ./anchor.env

echo ">> ANCHOR: height=$ANCHOR_HEIGHT digest=$ANCHOR_DIGEST"
echo ">> start order: anchor seat first, 15s head start, then the rest"

SSH="ssh -i /mnt/c/Users/Taj/.ssh/ovh_g1 -o ConnectTimeout=10"

# ---- edit order to match which seat held the anchor tip ----
echo ">> starting ovh-uk1"
$SSH ubuntu@51.195.201.74 "sudo systemctl start junoclaw-g1"
sleep 15

echo ">> starting home-vb"
systemctl start g1-node
sleep 5

echo
echo ">> external seats: Ravi starts vahana; seat-5 operator starts theirs."
echo ">> then run verify.sh"
