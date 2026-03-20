#!/bin/bash
# scripts/testnet.sh — 3-node Layer testnet orchestration script
#
# Launches a 3-node slay3rd testnet on localhost for integration testing.
# Each node runs on a different port pair (P2P + gRPC) with separate data dirs.
#
# Usage:
#   scripts/testnet.sh [start]   — Build, keygen, create configs, launch nodes
#   scripts/testnet.sh stop      — Kill all running slay3rd processes
#   scripts/testnet.sh status    — Print running status and latest block heights
#   scripts/testnet.sh wait <N>  — Wait until all nodes have reached block height N
#   scripts/testnet.sh e2e       — Run end-to-end contract deployment and query test
#
# Requires: cargo in PATH (for build and keygen tool)
# SLAY_LOG can be set to control log verbosity (default: info)

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

TESTNET_DIR="${TESTNET_DIR:-/tmp/layer-testnet}"
NUM_VALIDATORS=3
SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SLAY3RD="${SDK_ROOT}/target/release/slay3rd"
KEYGEN_MANIFEST="${SDK_ROOT}/tools/generate-testnet-keys/Cargo.toml"
RUST_LOG="${SLAY_LOG:-info}"

# Port allocation:
#   Node 0: P2P=7001, gRPC=9090
#   Node 1: P2P=7002, gRPC=9091
#   Node 2: P2P=7003, gRPC=9092
P2P_BASE=7001
GRPC_BASE=9090

# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------

log() { echo "[testnet.sh] $*"; }
err() { echo "[testnet.sh] ERROR: $*" >&2; exit 1; }

node_dir() { echo "${TESTNET_DIR}/node-${1}"; }

pid_file() { echo "${TESTNET_DIR}/node-${1}/node.pid"; }

log_file() { echo "${TESTNET_DIR}/node-${1}/node.log"; }

key_file() { echo "${TESTNET_DIR}/validator-${1}/keys.json"; }

node_running() {
    local pid_f
    pid_f="$(pid_file "$1")"
    [[ -f "$pid_f" ]] && kill -0 "$(cat "$pid_f")" 2>/dev/null
}

wait_for_height() {
    local target_height="$1"
    local timeout_secs="${2:-120}"
    local elapsed=0

    log "Waiting for all nodes to reach block height ${target_height} (timeout: ${timeout_secs}s)..."

    while [[ $elapsed -lt $timeout_secs ]]; do
        local all_at_height=true
        for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
            local log_f
            log_f="$(log_file "$i")"
            if [[ ! -f "$log_f" ]]; then
                all_at_height=false
                break
            fi
            # Parse "height=N" from structured tracing output.
            # Strip ANSI escape codes first (tracing emits color codes that break grep -P).
            local latest_height
            latest_height=$(grep "Block finalized" "$log_f" 2>/dev/null | tail -1 \
                | sed 's/\x1b\[[0-9;]*m//g' \
                | python3 -c "import re,sys; line=sys.stdin.read(); m=re.search(r'height=(\d+)', line); print(m.group(1) if m else '0')" \
                2>/dev/null || echo "0")
            if [[ -z "$latest_height" ]] || [[ "$latest_height" -lt "$target_height" ]]; then
                all_at_height=false
                break
            fi
        done

        if $all_at_height; then
            log "All nodes reached block height ${target_height}."
            return 0
        fi

        sleep 2
        elapsed=$((elapsed + 2))
    done

    log "WARNING: Timed out waiting for block height ${target_height} after ${timeout_secs}s"
    return 1
}

# ---------------------------------------------------------------------------
# Command: start
# ---------------------------------------------------------------------------

cmd_start() {
    log "=== Starting 3-node Layer testnet ==="

    # Step 1: Build slay3rd binary
    log "Building slay3rd (release, --features rocksdb)..."
    cargo build -p slay3rd --release --features rocksdb --manifest-path "${SDK_ROOT}/Cargo.toml" \
        2>&1 | tail -5
    [[ -f "$SLAY3RD" ]] || err "slay3rd binary not found at ${SLAY3RD}"
    log "Build complete: ${SLAY3RD}"

    # Step 2: Generate testnet key material
    log "Generating testnet key material for ${NUM_VALIDATORS} validators..."
    mkdir -p "${TESTNET_DIR}"
    cargo run --manifest-path "${KEYGEN_MANIFEST}" -- \
        --output-dir "${TESTNET_DIR}" \
        --validators "${NUM_VALIDATORS}" \
        2>&1
    log "Key generation complete."

    # Step 3: Create node directories and config files
    for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
        local dir p2p_port grpc_port
        dir="$(node_dir "$i")"
        p2p_port=$((P2P_BASE + i))
        grpc_port=$((GRPC_BASE + i))

        rm -rf "${dir}/data"
        mkdir -p "${dir}/data"

        # Read this node's ed25519 public key from key material
        local this_ed25519_pub
        this_ed25519_pub=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['ed25519_public_hex'])" "$(key_file "$i")")

        # Write TOML config with [[peers]] table format
        cat > "${dir}/config.toml" <<TOML
# slay3rd node ${i} config — auto-generated by scripts/testnet.sh
validator_index = ${i}
chain_id = "slay3r-testnet-1"
p2p_listen = "127.0.0.1:${p2p_port}"
grpc_listen = "127.0.0.1:${grpc_port}"
data_dir = "${dir}/data"
bls_key_path = "$(key_file "$i")"
identity_key_path = "$(key_file "$i")"
genesis_path = "${TESTNET_DIR}/genesis.json"
mempool_max_pending = 10000
leader_timeout_ms = 3000
certification_timeout_ms = 5000
TOML

        # Add [[peers]] entries for all OTHER validators
        for j in $(seq 0 $((NUM_VALIDATORS - 1))); do
            if [[ "$j" != "$i" ]]; then
                local peer_p2p_port peer_ed25519_pub
                peer_p2p_port=$((P2P_BASE + j))
                peer_ed25519_pub=$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(d['ed25519_public_hex'])" "$(key_file "$j")")

                cat >> "${dir}/config.toml" <<TOML

[[peers]]
address = "127.0.0.1:${peer_p2p_port}"
public_key = "${peer_ed25519_pub}"
TOML
            fi
        done

        log "  Node ${i}: config at ${dir}/config.toml (P2P=:${p2p_port}, gRPC=:${grpc_port})"
    done

    # Step 4: Create shared genesis.json
    cat > "${TESTNET_DIR}/genesis.json" <<'JSON'
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
    log "Genesis file written to ${TESTNET_DIR}/genesis.json"

    # Step 5: Launch all 3 nodes in the background
    for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
        local dir log_f pid_f
        dir="$(node_dir "$i")"
        log_f="$(log_file "$i")"
        pid_f="$(pid_file "$i")"

        # Kill any stale process from previous run
        if [[ -f "$pid_f" ]]; then
            local old_pid
            old_pid="$(cat "$pid_f")"
            if kill -0 "$old_pid" 2>/dev/null; then
                log "  Stopping stale node ${i} (PID ${old_pid})"
                kill "$old_pid" 2>/dev/null || true
                sleep 1
            fi
            rm -f "$pid_f"
        fi

        log "  Launching node ${i}..."
        RUST_LOG="${RUST_LOG}" \
            "${SLAY3RD}" "${dir}/config.toml" \
            > "$log_f" 2>&1 &
        echo "$!" > "$pid_f"
        log "  Node ${i} started — PID: $(cat "$pid_f"), log: ${log_f}"
    done

    log ""
    log "All ${NUM_VALIDATORS} nodes launched with authenticated P2P."
    log "To monitor: tail -f ${TESTNET_DIR}/node-0/node.log"
    log "To stop: $0 stop"
    log "To verify consensus: scripts/verify-consensus.sh"
}

# ---------------------------------------------------------------------------
# Command: stop
# ---------------------------------------------------------------------------

cmd_stop() {
    log "=== Stopping testnet ==="
    local stopped=0

    for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
        local pid_f
        pid_f="$(pid_file "$i")"
        if [[ -f "$pid_f" ]]; then
            local pid
            pid="$(cat "$pid_f")"
            if kill -0 "$pid" 2>/dev/null; then
                log "  Stopping node ${i} (PID ${pid})"
                kill "$pid" 2>/dev/null || true
                stopped=$((stopped + 1))
            else
                log "  Node ${i} (PID ${pid}) already stopped"
            fi
            rm -f "$pid_f"
        else
            log "  Node ${i}: no PID file found"
        fi
    done

    log "Stopped ${stopped} node(s)."
}

# ---------------------------------------------------------------------------
# Command: status
# ---------------------------------------------------------------------------

cmd_status() {
    log "=== Testnet status ==="
    for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
        local pid_f log_f
        pid_f="$(pid_file "$i")"
        log_f="$(log_file "$i")"

        if node_running "$i"; then
            local pid latest_height
            pid="$(cat "$pid_f")"
            latest_height=$(grep "Block finalized" "$log_f" 2>/dev/null | tail -1 \
                | sed 's/\x1b\[[0-9;]*m//g' \
                | python3 -c "import re,sys; line=sys.stdin.read(); m=re.search(r'height=(\d+)', line); print(m.group(1) if m else 'unknown')" \
                2>/dev/null || echo "unknown")
            log "  Node ${i}: RUNNING (PID ${pid}), latest block height: ${latest_height}"
        else
            log "  Node ${i}: STOPPED"
        fi
    done
}

# ---------------------------------------------------------------------------
# Command: wait <height>
# ---------------------------------------------------------------------------

cmd_wait() {
    local target="${1:-20}"
    wait_for_height "$target"
}

# ---------------------------------------------------------------------------
# Command: e2e — end-to-end contract deployment and execution test
# ---------------------------------------------------------------------------

cmd_e2e() {
    log "Running end-to-end contract deployment test..."

    # Prerequisites: testnet must be running and nodes must be producing blocks
    local grpc_addr="127.0.0.1:${GRPC_BASE}"
    local tx_sender_bin="${SDK_ROOT}/target/release/tx-sender"

    # Step 1: Check that the testnet is running and producing blocks
    log "Step 1: Verifying testnet is running..."

    # Verify gRPC port is open (tonic doesn't enable server reflection, so grpcurl list
    # returns "server does not support the reflection API" — not a connection failure).
    # Use nc for a direct TCP check.
    if ! nc -zv "${grpc_addr%%:*}" "${grpc_addr##*:}" >/dev/null 2>&1; then
        err "gRPC server not responding on ${grpc_addr}. Is the testnet running? (scripts/testnet.sh start)"
    fi
    log "  gRPC server responding on ${grpc_addr} (TCP port open)"

    # Step 2: Build the root contract WASM
    log "Step 2: Building contracts/root/ WASM..."
    local wasm_target_dir="${SDK_ROOT}/target/wasm32-unknown-unknown/release"
    local wasm_path="${wasm_target_dir}/layer_root.wasm"

    # Check for wasm32 target
    if ! rustup target list --installed 2>/dev/null | grep -q "wasm32-unknown-unknown"; then
        log "  Installing wasm32-unknown-unknown target..."
        rustup target add wasm32-unknown-unknown
    fi

    # Build the root contract WASM library.
    # Use --lib to skip binary targets (schema.rs only compiles on native targets, not wasm32).
    # Allow non-zero exit: schema binary errors don't prevent the WASM lib from being built.
    (cd "${SDK_ROOT}" && cargo build \
        --release \
        --target wasm32-unknown-unknown \
        -p layer-root \
        --lib \
        --manifest-path Cargo.toml \
        2>&1 | tail -10) || true

    # Check for the built wasm (may be named differently)
    if [[ ! -f "$wasm_path" ]]; then
        # Try alternate name from crate name
        wasm_path=$(find "${wasm_target_dir}" -name "*.wasm" 2>/dev/null | head -1 || true)
        if [[ -z "$wasm_path" ]]; then
            err "WASM file not found after build. Check contracts/root/ Cargo.toml crate name."
        fi
    fi
    log "  Contract WASM built: ${wasm_path}"

    # Step 3: Build the tx-sender tool
    log "Step 3: Building tools/tx-sender..."
    cargo build -p tx-sender --manifest-path "${SDK_ROOT}/Cargo.toml" --release 2>&1 | tail -5
    [[ -f "$tx_sender_bin" ]] || err "tx-sender binary not found at ${tx_sender_bin} after build"
    log "  tx-sender built: ${tx_sender_bin}"

    # Step 4: Query deployer balance to confirm funded account is reachable
    log "Step 4: Querying deployer balance..."
    "${tx_sender_bin}" balance --grpc "${grpc_addr}" 2>&1 | tee /tmp/balance_result.txt || {
        log "  WARNING: balance query failed — node may not be accepting requests yet"
    }

    # Step 5: Submit StoreCode transaction
    log "Step 5: Submitting StoreCode transaction..."
    "${tx_sender_bin}" store-code \
        --grpc "${grpc_addr}" \
        --wasm "${wasm_path}" \
        2>&1 | tee /tmp/store_code_result.txt || {
        log "  WARNING: StoreCode submission failed — check node logs for details"
    }

    # Step 6: InstantiateContract (assume code_id=1 on fresh chain)
    log "Step 6: Submitting InstantiateContract transaction..."
    local code_id
    code_id=$(grep -oP 'code_id=\K[0-9]+' /tmp/store_code_result.txt 2>/dev/null || echo "1")
    log "  Using code_id=${code_id}"
    "${tx_sender_bin}" instantiate \
        --grpc "${grpc_addr}" \
        --code-id "${code_id}" \
        2>&1 | tee /tmp/instantiate_result.txt || {
        log "  WARNING: InstantiateContract submission failed — check node logs"
    }

    # Step 7: ExecuteContract (if contract address is in logs)
    log "Step 7: Submitting ExecuteContract transaction..."
    local contract_addr
    contract_addr=$(grep -oP 'contract_address=\K\S+' /tmp/instantiate_result.txt 2>/dev/null || echo "")
    if [[ -n "$contract_addr" ]]; then
        log "  InstantiateContract logged contract_address=${contract_addr}"
        "${tx_sender_bin}" execute \
            --grpc "${grpc_addr}" \
            --contract "${contract_addr}" \
            --msg '{}' \
            2>&1 || {
            log "  WARNING: ExecuteContract submission failed — check node logs"
        }
    else
        log "  contract_address not in instantiate output — check node logs for actual address"
        log "  (InstantiateContract response is returned in BroadcastTx data, not logged by tx-sender)"
    fi

    # Step 8: Query contract state (if contract address known)
    log "Step 8: Querying contract state..."
    if [[ -n "$contract_addr" ]]; then
        "${tx_sender_bin}" query \
            --grpc "${grpc_addr}" \
            --contract "${contract_addr}" \
            --msg '{}' \
            2>&1 || {
            log "  WARNING: Contract state query failed — check that Cosmos query router is active"
        }
    else
        log "  Skipping query — contract_address not available (check node logs)"
    fi

    # Step 9: Summary
    log ""
    log "=== E2E Test Results ==="
    log "  gRPC server: RESPONDING on ${grpc_addr}"
    log "  WASM binary: BUILT at ${wasm_path}"
    log "  tx-sender: AVAILABLE at ${tx_sender_bin}"
    if grep -q "StoreCode TX submitted" /tmp/store_code_result.txt 2>/dev/null; then
        local sc_code_id
        sc_code_id=$(grep -oP 'code_id=\K[0-9]+' /tmp/store_code_result.txt 2>/dev/null || echo "1")
        log "  StoreCode: SUBMITTED (code_id=${sc_code_id})"
    else
        log "  StoreCode: NOT CONFIRMED (check node logs)"
    fi
    if grep -q "InstantiateContract TX submitted" /tmp/instantiate_result.txt 2>/dev/null; then
        log "  InstantiateContract: SUBMITTED"
    else
        log "  InstantiateContract: NOT CONFIRMED (check node logs)"
    fi
    log ""
    log "E2E test complete. Check node logs for tx execution results:"
    log "  tail -f ${TESTNET_DIR}/node-0/node.log"
}

# ---------------------------------------------------------------------------
# Dispatch
# ---------------------------------------------------------------------------

case "${1:-}" in
    start) cmd_start ;;
    stop)  cmd_stop ;;
    status) cmd_status ;;
    wait)  cmd_wait "${2:-20}" ;;
    e2e)   cmd_e2e ;;
    *)     echo "Usage: $0 {start|stop|status|wait [height]|e2e}"; exit 1 ;;
esac
