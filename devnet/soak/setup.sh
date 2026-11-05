#!/bin/bash
# devnet soak setup — stages 4 node dirs under /tmp/jcl-soak (WSL).
# Deterministic seed-0 devnet keys only; never run against live chain.
set -euo pipefail

SOAK=/tmp/jcl-soak
SRC=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/testnet-keys

mkdir -p "$SOAK/bin"
for i in 0 1 2 3; do
    mkdir -p "$SOAK/node$i/data"
    cp "$SRC/validator-$i/keys.json" "$SOAK/node$i/keys.json"
done

# Peer pubkeys (from devnet/config/*.toml peer tables — public data only)
P0="b371e8d4c1a35a376a13772d763f207c892421ac237262451cdf448e1dd1b25f"
P1="563757f5310c035a30d16c33cc186474b07451e2434d1e573cd506bc3e22451e"
P2="28833a25d511f2e03efca77d87ddc3eedd5f5181adb947143d17e2e87e2fa685"
P3="5d25ada62d8910a9e43e3711c5b581b6a0860aa338954fe65a847eea8d48c1a2"
PUBS=("$P0" "$P1" "$P2" "$P3")

# Bind the WSL eth0 address, not loopback — the commonware lookup tracker
# does not dial 127.0.0.0/8 (same reason the docker devnet uses 172.28.0.x).
HOST_IP="$(ip -4 -o addr show eth0 | awk '{split($4,a,"/"); print a[1]}')"
echo "HOST_IP=$HOST_IP"

for i in 0 1 2 3; do
    p2p=$((7100 + i))
    grpc=$((9190 + i))
    cfg="$SOAK/node$i/node.toml"
    {
        echo "validator_index = $i"
        echo 'chain_id = "junoclaw-1"'
        echo "p2p_listen = \"$HOST_IP:$p2p\""
        echo "grpc_listen = \"127.0.0.1:$grpc\""
        echo "bls_key_path = \"$SOAK/node$i/keys.json\""
        echo "identity_key_path = \"$SOAK/node$i/keys.json\""
        echo "data_dir = \"$SOAK/node$i/data\""
        echo 'genesis_path = ""'
        echo "mempool_max_pending = 10000"
        echo "leader_timeout_ms = 3000"
        echo "certification_timeout_ms = 5000"
        echo "hybrid_consensus = true"
        echo "insecure_devnet = true"
        echo ""
        for j in 0 1 2 3; do
            [ "$j" = "$i" ] && continue
            pp=$((7100 + j))
            echo "[[peers]]"
            echo "address = \"$HOST_IP:$pp\""
            echo "public_key = \"${PUBS[$j]}\""
            echo ""
        done
    } > "$cfg"
done

echo "SETUP-OK"
ls -la "$SOAK/node0/"
