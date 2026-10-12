#!/bin/bash
# probe-deploy2.sh - dump deployment version fields + compute version CLI generates for current SDL
set -e
python3 - <<'PY'
import json
d = json.load(open('/tmp/dep.json'))
def walk(o, path=""):
    if isinstance(o, dict):
        for k,v in o.items():
            if k.lower() in ("version","hash") and not isinstance(v,(dict,list)):
                print(f"{path}.{k} = {v}")
            walk(v, f"{path}.{k}")
    elif isinstance(o, list):
        for i,v in enumerate(o):
            walk(v, f"{path}[{i}]")
walk(d)
print("TOP KEYS:", list(d.keys()))
print("DEP KEYS:", list(d.get('deployment',{}).keys()))
PY
echo "=== generate-only update (version CLI computes from current SDL) ==="
SDL=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony/_secrets/deploy-akash-seat.yaml
akash tx deployment update "$SDL" --dseq 28973001 --owner akash1eehlc3mu8tdkhp7pc4whjkpkw329j9sq6l05dt --generate-only --chain-id akashnet-2 2>&1 | python3 -c 'import json,sys; s=sys.stdin.read(); print(s[:2000])'
