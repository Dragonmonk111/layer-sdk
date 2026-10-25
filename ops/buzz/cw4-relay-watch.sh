#!/bin/bash
# cw4-relay-watch.sh — sync relay pubkey_allowlist with DAO cw4 membership.
# Members source:  /api/wasm/smart/<group>?query={list_members:{}}
# Binding source:  /opt/buzz/state/members.json  {"juno1...": "<npub hex>"}
# Safety: only manages pubkeys it synced before (state/synced.json) or
# new members — never removes manual/agent entries. No-ops if the DAO
# query fails.
set -e
B=/opt/buzz/state
GROUP=$(cat $B/group_addr 2>/dev/null || true)
[ -n "$GROUP" ] || exit 0          # no DAO group configured yet
Q=$(python3 -c 'import base64,sys;print(base64.urlsafe_b64encode(b"{\"list_members\":{}}").decode().rstrip("="))')
RESP=$(curl -fsS -m 10 "http://127.0.0.1:8082/api/wasm/smart/$GROUP?query=$Q" 2>/dev/null) || exit 0
ADDRS=$(echo "$RESP" | python3 -c 'import json,sys;print(" ".join(m["addr"] for m in json.load(sys.stdin)["data"]["members"]))' 2>/dev/null) || exit 0
[ -n "$ADDRS" ] || exit 0

touch $B/members.json
WANT=$(python3 - "$ADDRS" <<PY
import json,sys
m=json.load(open('$B/members.json'))
print(' '.join(m[a] for a in sys.argv[1].split() if a in m))
PY
)
for hex in $WANT; do
  docker exec buzz-postgres psql -U buzz -d buzz -qc "INSERT INTO pubkey_allowlist (community_id,pubkey,note)
     SELECT id,decode('$hex','hex'),'dao-member'
     FROM communities WHERE host='relay.junoclaw.xyz' ON CONFLICT DO NOTHING"     && echo "$(date -Is) synced $hex" >> $B/watch.log
done
echo "$WANT" > $B/synced.json
# removals: synced-before but no longer wanted
if [ -f $B/prev_synced ]; then
  for hex in $(cat $B/prev_synced); do
    if ! echo "$WANT" | grep -q "$hex"; then
      docker exec buzz-postgres psql -U buzz -d buzz -qc "DELETE FROM pubkey_allowlist WHERE pubkey=decode('$hex','hex') AND note='dao-member'" && echo "$(date -Is) removed $hex" >> $B/watch.log
    fi
  done
fi
cp $B/synced.json $B/prev_synced
