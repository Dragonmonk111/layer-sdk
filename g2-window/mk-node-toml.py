#!/usr/bin/env python3
"""Generate a G2 node.toml for a seat from ceremony/shared.json.

Usage: mk-node-toml.py <shared.json> <moniker> <anchor_height> <anchor_digest> [old_node.toml]

Peers = every OTHER validator in the set (sorted order = validator_index).
If old_node.toml is given, its listen paths/data_dir are preserved.
"""
import json
import re
import sys

shared = json.load(open(sys.argv[1]))
moniker = sys.argv[2]
anchor_h = sys.argv[3]
anchor_d = sys.argv[4]
old_toml = open(sys.argv[5]).read() if len(sys.argv) > 5 else ""

me = None
for v in shared["validators"]:
    if v["moniker"] == moniker:
        me = v
        break
if me is None:
    sys.exit(f"moniker {moniker!r} not in shared.json — have: "
             + str([v["moniker"] for v in shared["validators"]]))

def keep(key, default):
    m = re.search(rf'^{key} = (.+)$', old_toml, re.M)
    return m.group(1) if m else default

out = f"""validator_index = {me['validator_index']}
chain_id = "{shared['chain_id']}"
p2p_listen = {keep('p2p_listen', '"0.0.0.0:7001"')}
grpc_listen = {keep('grpc_listen', '"0.0.0.0:9090"')}
bls_key_path = {keep('bls_key_path', '"/keys/keys.json"')}
identity_key_path = {keep('identity_key_path', '"/keys/keys.json"')}
data_dir = {keep('data_dir', '"/data"')}
genesis_path = {keep('genesis_path', '"/config/genesis.json"')}
mempool_max_pending = 10000
leader_timeout_ms = 3000
certification_timeout_ms = 5000
pruning = "validator"
hybrid_consensus = true
restart_anchor = {{ height = {anchor_h}, digest = "{anchor_d}" }}
"""
for v in shared["validators"]:
    if v["validator_index"] == me["validator_index"]:
        continue
    out += f'\n[[peers]]\npublic_key = "{v["ed25519_public_hex"]}"\naddress = "{v["p2p_address"]}"\n'

sys.stdout.write(out)
