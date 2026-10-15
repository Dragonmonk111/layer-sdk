#!/usr/bin/env bash
# Phase B step 5 — read each seat's last EXECUTED tip after halt.
# Anchor = highest (height, digest) observed; all seats must agree on it.
# For seats we can't SSH (vahana), the operator runs the same query and
# reports the height+digest; coordinator enters it in anchor.env by hand.
set -euo pipefail
cd "$(dirname "$0")"

SSH="ssh -i /mnt/c/Users/Taj/.ssh/ovh_g1 -o ConnectTimeout=10"
OUT=anchor.env

tip_from_log() { # $1 = log path on its host (read via the same shell)
  grep -aoE 'height=[0-9]+ app_hash=[0-9a-f]+ digest=[0-9a-f]+' "$1" | tail -1
}

echo ">> ovh-uk1 executed tip:"
ovh_line=$($SSH ubuntu@51.195.201.74 \
  "journalctl -u junoclaw-g1 --no-pager | grep -aoE 'height=[0-9]+ app_hash=[0-9a-f]+ digest=[0-9a-f]+' | tail -1")
echo "   $ovh_line"

echo ">> home-vb executed tip:"
vb_line=$(tip_from_log /opt/g1-node/slay3rd.log)
echo "   $vb_line"

echo
echo ">> vahana: ask Ravi for his last 'Block finalized' line and paste below."
echo
echo ">> choose ANCHOR = highest height among all seats; if heights tie,"
echo "   digests MUST match (a mismatch = divergence — STOP and investigate)."
echo
cat > "$OUT" <<EOF
# G2 restart anchor — fill after comparing all seats' executed tips.
# ANCHOR_HEIGHT=<highest height>
# ANCHOR_DIGEST=<its payload digest>
EOF
echo "wrote $OUT — fill it in, then run apply.sh"
