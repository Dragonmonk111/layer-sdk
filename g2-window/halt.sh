#!/usr/bin/env bash
# Phase B step 4 — coordinated halt of the live G1 seats we control.
# External seats (vahana/Ravi, seat-5 operator) must halt theirs at the
# agreed time — this script prints the checklist for them.
set -euo pipefail

SSH="ssh -i /mnt/c/Users/Taj/.ssh/ovh_g1 -o ConnectTimeout=10"

echo ">> halting ovh-uk1"
$SSH ubuntu@51.195.201.74 "sudo systemctl stop junoclaw-g1 && systemctl is-active junoclaw-g1 || echo stopped"

echo ">> halting home-vb"
systemctl stop g1-node && (systemctl is-active g1-node || echo stopped)

echo
echo ">> EXTERNAL SEATS — confirm each is stopped before proceeding:"
echo "   vahana (Ravi):  sudo systemctl stop <his unit>"
echo "   seat-5 (new op): not yet running — nothing to stop"
echo
echo ">> give peers ~30s to settle, then run read-tips.sh"
