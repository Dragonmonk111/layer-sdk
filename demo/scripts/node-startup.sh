#!/bin/bash
# demo/scripts/node-startup.sh
#
# Demonstrates the slay3rd node startup sequence and documents the Phase 2
# BindFailed panic.
#
# Phase 2 limitation: commonware_p2p::simulated generates a random IPv4
# address (OsRng.next_u32() → Ipv4Addr::from_bits) and calls
# TcpListener::bind(random_ip). On macOS that IP is not assigned to any local
# interface, so the bind fails with EADDRNOTAVAIL → BindFailed panic.
#
# The simulated network was designed for commonware_runtime::deterministic
# (a fake test networking layer). Phase 2 pairs it with
# commonware_runtime::tokio, which tries to make real TCP sockets.
#
# Phase 3 fix: replace commonware_p2p::simulated with
# commonware_p2p::authenticated, which uses real TCP with Ed25519-
# authenticated channels and works correctly with the tokio runtime.
#
# This script is a preview of what the startup flow will look like in Phase 3.

set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEMO_DIR="${1:-/tmp/layer-demo}"
VALIDATOR_INDEX="${2:-0}"

# Check binary is built
BINARY="${SDK_ROOT}/target/release/slay3rd"
if [[ ! -f "$BINARY" ]]; then
    echo "slay3rd not built. Building now..."
    cargo build -p slay3rd --release --manifest-path "${SDK_ROOT}/Cargo.toml"
fi

# Generate keys if not present
if [[ ! -f "${DEMO_DIR}/validator-0/keys.json" ]]; then
    echo "Generating testnet keys..."
    bash "$(dirname "${BASH_SOURCE[0]}")/keygen-and-inspect.sh" "$DEMO_DIR"
fi

# Create node directory
NODE_DIR="${DEMO_DIR}/node-${VALIDATOR_INDEX}"
mkdir -p "${NODE_DIR}/wal"

# Write genesis
cat > "${DEMO_DIR}/genesis.json" <<'JSON'
{
  "chain_id": "slay3r-testnet-1",
  "initial_height": 1,
  "app_state": {
    "bank": [],
    "wasm": {
      "gov_account": "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug"
    }
  }
}
JSON

# Write node config
P2P_PORT=$((26656 + VALIDATOR_INDEX))
GRPC_PORT=$((9090 + VALIDATOR_INDEX))

# Build peers list
PEERS=""
for j in 0 1 2; do
    if [[ "$j" != "$VALIDATOR_INDEX" ]]; then
        PEER_PORT=$((26656 + j))
        if [[ -n "$PEERS" ]]; then
            PEERS="${PEERS}, \"127.0.0.1:${PEER_PORT}\""
        else
            PEERS="\"127.0.0.1:${PEER_PORT}\""
        fi
    fi
done

cat > "${NODE_DIR}/config.toml" <<TOML
# slay3rd node ${VALIDATOR_INDEX} config (demo)
validator_index = ${VALIDATOR_INDEX}
chain_id = "slay3r-testnet-1"
p2p_listen = "127.0.0.1:${P2P_PORT}"
grpc_listen = "127.0.0.1:${GRPC_PORT}"
peers = [${PEERS}]
bls_key_path = "${DEMO_DIR}/validator-${VALIDATOR_INDEX}/keys.json"
identity_key_path = "${DEMO_DIR}/validator-${VALIDATOR_INDEX}/keys.json"
wal_path = "${NODE_DIR}/wal"
genesis_path = "${DEMO_DIR}/genesis.json"
mempool_max_pending = 10000
leader_timeout_ms = 3000
certification_timeout_ms = 5000
TOML

echo "=== Starting slay3rd (validator ${VALIDATOR_INDEX}) ==="
echo ""
echo "  Config:  ${NODE_DIR}/config.toml"
echo "  Keys:    ${DEMO_DIR}/validator-${VALIDATOR_INDEX}/keys.json"
echo "  P2P:     127.0.0.1:${P2P_PORT}"
echo "  gRPC:    127.0.0.1:${GRPC_PORT} (stub — not yet wired)"
echo ""
echo "  ⚠  Phase 2 known issue: this node will PANIC with BindFailed."
echo "     commonware_p2p::simulated calls TcpListener::bind(random_ip)"
echo "     which fails on macOS because the IP is not a local interface."
echo "     Phase 3 switches to commonware_p2p::authenticated (real TCP)."
echo ""
echo "  Expected log before panic:"
echo "    INFO slay3rd: starting Layer Commonware consensus node"
echo "    INFO BLS key material loaded"
echo "    INFO BLS12-381 threshold signing scheme initialized"
echo "    INFO P2P simulated network initialized (3 validators)"
echo "    ERROR commonware_runtime::utils::handle: task panicked BindFailed"
echo ""
echo "  Uncomment the RUST_LOG line below to actually run it (it will panic)."
echo "  See demo/README.md Part 6 for the full explanation."
echo ""

# Uncomment to run (will panic with BindFailed in Phase 2):
# RUST_LOG="${RUST_LOG:-info}" "${BINARY}" "${NODE_DIR}/config.toml"
echo "(node not started — BindFailed expected, see README Part 6)"
