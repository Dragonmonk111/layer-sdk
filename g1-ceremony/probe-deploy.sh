#!/bin/bash
# probe-deploy.sh - inspect on-chain deployment version + group spec for dseq 28973001
set -e
NODE=https://rpc.akashnet.net:443
OWNER=akash1eehlc3mu8tdkhp7pc4whjkpkw329j9sq6l05dt
DSEQ=28973001
akash query deployment get --dseq $DSEQ --owner $OWNER --node $NODE -o json > /tmp/dep.json 2>/dev/null
python3 - <<'PY'
import json, base64
d = json.load(open('/tmp/dep.json'))
dep = d.get('deployment', {})
print("STATE:", dep.get('state'))
v = dep.get('version')
try:
    print("VERSION(b64):", v)
    print("VERSION(hex):", base64.b64decode(v).hex() if v else None)
except Exception as e:
    print("version decode err", e)
groups = d.get('groups', [])
print("NGROUPS:", len(groups))
for g in groups:
    gs = g.get('group_spec', {})
    print("--- group_spec ---")
    print(json.dumps(gs, indent=1)[:3000])
PY
