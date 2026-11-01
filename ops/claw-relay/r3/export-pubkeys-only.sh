#!/bin/bash
# export-pubkeys-only.sh — run ON THE SEAT HOST where keys.json lives.
# Writes ONLY public fields to pubkeys-only.json next to it. Private fields
# (bls_private_hex, ed25519_private_hex, mayo_private_hex, share_hex,
# sharing_hex) are never read out, never printed, never leave the host.
# Usage: bash export-pubkeys-only.sh /path/to/keys.json /path/to/pubkeys-only.json
set -euo pipefail
KEYS="${1:?usage: export-pubkeys-only.sh keys.json out.json}"
OUT="${2:?usage: export-pubkeys-only.sh keys.json out.json}"
python3 - "$KEYS" "$OUT" <<'PY'
import json, sys, os
keys_path, out_path = sys.argv[1], sys.argv[2]
k = json.load(open(keys_path))
pub = {
    "validator_index":   k["validator_index"],
    "ed25519_public_hex": k["ed25519_public_hex"],
    "bls_public_hex":    k["bls_public_hex"],
    "mayo_public_hex":   k.get("mayo_public_hex"),
    "mayo_variant":      k.get("mayo_variant"),
    "threshold_public_key_hex": k.get("threshold_public_key_hex"),
}
# refuse to write next to dealer.seed or inside a ceremony bundle dir
if os.path.exists(os.path.join(os.path.dirname(keys_path), "dealer.seed")):
    sys.exit("dealer.seed present in key dir — export to a scratch dir instead")
with open(out_path, "w") as f:
    json.dump(pub, f, indent=2)
os.chmod(out_path, 0o644)
print(f"pubkeys-only.json written: {out_path} ({len(pub)} fields, no private material)")
PY
