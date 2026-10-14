#!/usr/bin/env bash
# Restage a ceremony window's output into ip-<octet>/ dirs that the compose
# file mounts. Usage: stage-window.sh <wN> — expects $R/ceremony-<wN>/shared.json
# and finalized $R/op-<i>/keys.json files (from the ceremony script).
set -euo pipefail
W=${1:?usage: stage-window.sh wN}
R=/root/r9
OUT=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/r9-rehearsal

rm -rf "$OUT/$W"
python3 - "$R/ceremony-$W" "$R" "$OUT/$W" <<'EOF'
import json, shutil, sys, os
ceremony, R, out = sys.argv[1], sys.argv[2], sys.argv[3]
data = json.load(open(f"{ceremony}/shared.json"))
for v in data['validators']:
    ip_octet = v['p2p_address'].split(':')[0].split('.')[-1]
    op = int(ip_octet) - 10                       # op-<i> dirs are named by IP order
    vi = v['validator_index']
    dst = os.path.join(out, f"ip-{ip_octet}")
    os.makedirs(dst, exist_ok=True)
    shutil.copy(f"{R}/op-{op}/keys.json", f"{dst}/keys.json")
    shutil.copy(f"{ceremony}/validator-{vi}/node-{vi}.toml", f"{dst}/node.toml")
    # devnet patch: empty genesis + insecure_devnet + archive pruning
    t = open(f"{dst}/node.toml").read()
    t = t.replace('genesis_path = "/config/genesis.json"', 'genesis_path = ""')
    t = t.replace('# insecure_devnet must stay unset on a real network', 'insecure_devnet = true')
    t = t.replace('pruning = "validator"', 'pruning = "archive"')
    open(f"{dst}/node.toml", 'w').write(t)
    print(f"ip-{ip_octet}: validator_index={vi} moniker={v['moniker']}")
EOF
