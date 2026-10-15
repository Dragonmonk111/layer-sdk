#!/usr/bin/env bash
# Phase C — verify the G2 set is live and the new seat participates.
set -euo pipefail

SSH="ssh -i /mnt/c/Users/Taj/.ssh/ovh_g1 -o ConnectTimeout=10"

echo "== finality =="
$SSH ubuntu@51.195.201.74 "journalctl -u junoclaw-g1 -n 200 --no-pager | grep -oE 'height=[0-9]+' | tail -3"
tail -50 /opt/g1-node/slay3rd.log | grep -oE 'height=[0-9]+' | tail -3

echo
echo "== cert size (n=5 envelope should differ from n=4) =="
$SSH ubuntu@51.195.201.74 "journalctl -u junoclaw-g1 -n 500 --no-pager | grep -oE 'cert_len=[0-9]+' | tail -1"

echo
echo "== peer counts =="
$SSH ubuntu@51.195.201.74 "journalctl -u junoclaw-g1 -n 500 --no-pager | grep -oE 'peers=[0-9]+' | tail -1"

echo
echo "== new seat proposing? (look for 'propose'/'built BlockPayload' on seat-5 log) =="
echo "   check the seat-5 host log directly"

echo
echo "== faucet smoke =="
curl -s -m 10 https://faucet.junoclaw.xyz/health
echo

echo "== PASS if: heights advancing, cert_len changed to the n=5 envelope,"
echo "   peers = n-1, new seat proposes within ~2min, faucet health ok =="
