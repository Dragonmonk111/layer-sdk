#!/bin/bash
# peer-map2.sh - extract validator peer IDs from shared.json + keys.json, map to seats
BASE=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony
OUT=/tmp/peer-map2.txt
{
echo "=== shared.json (validator identities) ==="
python3 -c "
import json
d = json.load(open('$BASE/ceremony-out/shared.json'))
print(json.dumps(d, indent=1)[:4000])
"
echo
echo "=== akash-seat/keys.json ==="
python3 -c "
import json
d = json.load(open('$BASE/akash-seat/keys.json'))
print(json.dumps(d, indent=1)[:2000])
" 2>&1
echo
echo "=== match failing peers ==="
for pid in c98739580820987b7d52e6b0a995e9ac9ad1cf9dec03457d2cab41f68d98846f 62e51f5e242b142ae706ee7cecc406be0da66b025b8bb6f4512a0028440de2bc; do
  echo \"--- \$pid\"
  grep -l \"\$pid\" \"$BASE\"/ceremony-out/shared.json \"$BASE\"/*/keys.json \"$BASE\"/*/node-*.toml 2>/dev/null
done
} > \"$OUT\" 2>&1
cat \"$OUT\"
