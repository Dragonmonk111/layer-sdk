import json, sys, os
BASE = r"/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony"
d = json.load(open(os.path.join(BASE, "ceremony-out/shared.json")))
targets = ["c98739580820987b", "62e51f5e242b142ae7"]

def walk(o, path=""):
    if isinstance(o, dict):
        for k, v in o.items():
            walk(v, path + "/" + str(k))
    elif isinstance(o, list):
        for i, v in enumerate(o):
            walk(v, path + "[" + str(i) + "]")
    elif isinstance(o, str):
        for t in targets:
            if t in o:
                print(path, "=", o[:80])

print("=== top-level keys ===")
print(list(d.keys()) if isinstance(d, dict) else type(d))
print("=== matches ===")
walk(d)

# also dump validator list structure briefly
for k in ("validators", "peers", "participants", "signers"):
    if k in d:
        print(f"--- {k} ---")
        print(json.dumps(d[k], indent=1)[:3000])
