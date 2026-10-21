#!/usr/bin/env python3
"""Generate a G2 node.toml for a seat from ceremony/shared.json.

Usage: mk-node-toml.py <shared.json> <moniker> <anchor_height> <anchor_digest> \
         [old_node.toml] [--state-sync-donors host:9090,host2:9090]

Peers = every OTHER validator in the set (sorted order = validator_index).
If old_node.toml is given, its listen paths/data_dir are preserved.
--state-sync-donors emits a [state_sync] block — REQUIRED for new seats
(lesson 7/G2: a fresh node without it cannot verify proposals, and
state_sync only fires on NoStoredState so it must be in the FIRST toml
the seat ever boots). Donor gRPC :9090 must be open to the joiner's IP
before the window.
"""
import json
import re
import sys

args = sys.argv[1:]
donors = None
if "--state-sync-donors" in args:
    i = args.index("--state-sync-donors")
    donors = [d.strip() for d in args[i + 1].split(",") if d.strip()]
    del args[i:i + 2]

shared = json.load(open(args[0]))
moniker = args[1]
anchor_h = args[2]
anchor_d = args[3]
old_toml = open(args[4]).read() if len(args) > 4 else ""

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

if donors:
    peer_lines = "\n".join(f'  "{d}",' for d in donors)
    out += f"""
[state_sync]
peers = [
{peer_lines}
]
# distinct donors that must advertise the identical certified anchor
# (Byzantine-divergence gate - never set below 2 with >=2 donors)
min_anchor_agree = {min(2, len(donors))}
"""

sys.stdout.write(out)
