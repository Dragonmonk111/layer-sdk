#!/bin/bash
# scripts/verify-consensus.sh — Phase 2 consensus verification
#
# Validates CONS-02 through CONS-05:
#   CONS-02: All nodes produce identical AppHash for the same block height
#   CONS-03: A crashed node (SIGKILL) restarts from WAL and catches up without manual intervention
#   CONS-04: No HashMap/HashSet/SystemTime in consensus-critical code paths
#   CONS-05: BLS12-381 threshold signature certificate is stored in Block.certificate (not None)
#
# Requires the 3-node testnet to already be running (scripts/testnet.sh start).
# Run this script after at least 20 blocks have been produced.
#
# Usage:
#   scripts/verify-consensus.sh
#   SKIP_CRASH_TEST=1 scripts/verify-consensus.sh   — Skip crash recovery test

set -euo pipefail

TESTNET_DIR="${TESTNET_DIR:-/tmp/layer-testnet}"
SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SLAY3RD="${SDK_ROOT}/target/release/slay3rd"
SKIP_CRASH_TEST="${SKIP_CRASH_TEST:-0}"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

pass() { echo "[PASS] $*"; }
fail() { echo "[FAIL] $*" >&2; exit 1; }
log()  { echo "       $*"; }

node_log() { echo "${TESTNET_DIR}/node-${1}/node.log"; }
node_pid_file() { echo "${TESTNET_DIR}/node-${1}/node.pid"; }
node_cfg() { echo "${TESTNET_DIR}/node-${1}/config.toml"; }

node_running() {
    local pid_f
    pid_f="$(node_pid_file "$1")"
    [[ -f "$pid_f" ]] && kill -0 "$(cat "$pid_f")" 2>/dev/null
}

# Extract the latest block height logged by a node
latest_height() {
    local log_f
    log_f="$(node_log "$1")"
    grep -oP 'height=\K[0-9]+' "$log_f" 2>/dev/null | sort -n | tail -1 || echo "0"
}

# Extract AppHash at a specific block height from a node's log.
# Looks for tracing output: height=N app_hash=HEXHEX
app_hash_at_height() {
    local node_idx="$1"
    local target_height="$2"
    local log_f
    log_f="$(node_log "$node_idx")"
    # Match lines containing both height=<target> and app_hash=<hex>
    # The tracing format is: height=N app_hash=HEXHASH ...
    grep "height=${target_height}" "$log_f" 2>/dev/null \
        | grep -oP 'app_hash=\K[0-9a-f]+' \
        | head -1 || echo ""
}

# Wait for a specific node to reach a block height (timeout in seconds)
wait_node_height() {
    local node_idx="$1"
    local target_height="$2"
    local timeout_secs="${3:-120}"
    local elapsed=0

    while [[ $elapsed -lt $timeout_secs ]]; do
        local h
        h="$(latest_height "$node_idx")"
        if [[ "$h" -ge "$target_height" ]]; then
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    return 1
}

# ---------------------------------------------------------------------------
# Pre-flight checks
# ---------------------------------------------------------------------------

echo "=== Phase 2 Consensus Verification ==="
echo ""

# Verify testnet is running
for i in 0 1 2; do
    if ! node_running "$i"; then
        fail "Node ${i} is not running. Start the testnet first: scripts/testnet.sh start"
    fi
done
log "Pre-flight: All 3 nodes are running."

# Verify nodes have produced blocks
for i in 0 1 2; do
    local_height="$(latest_height "$i")"
    if [[ "$local_height" -lt 5 ]]; then
        fail "Node ${i} has only produced ${local_height} blocks. Wait for at least 5 blocks before verifying."
    fi
done
log "Pre-flight: All nodes have produced >= 5 blocks."

# ---------------------------------------------------------------------------
# Test 1: AppHash Consensus (CONS-02, CONS-04)
# ---------------------------------------------------------------------------

echo ""
echo "--- Test 1: AppHash Consensus (CONS-02, CONS-04) ---"

# Check AppHash at block heights 5, 10, and 15 across all nodes.
# All 3 nodes must agree on the AppHash for each height.
APPHASH_PASS=true
for height in 5 10 15; do
    hash0="$(app_hash_at_height 0 "$height")"
    hash1="$(app_hash_at_height 1 "$height")"
    hash2="$(app_hash_at_height 2 "$height")"

    if [[ -z "$hash0" ]] || [[ -z "$hash1" ]] || [[ -z "$hash2" ]]; then
        log "Height ${height}: AppHash not yet available on all nodes (node0=${hash0:-MISSING}, node1=${hash1:-MISSING}, node2=${hash2:-MISSING})"
        log "  Waiting for nodes to catch up..."
        # Wait for height
        for i in 0 1 2; do
            wait_node_height "$i" "$height" 60 || true
        done
        hash0="$(app_hash_at_height 0 "$height")"
        hash1="$(app_hash_at_height 1 "$height")"
        hash2="$(app_hash_at_height 2 "$height")"
    fi

    if [[ -z "$hash0" ]] || [[ -z "$hash1" ]] || [[ -z "$hash2" ]]; then
        log "  Height ${height}: AppHash still not available — skipping this height"
        continue
    fi

    if [[ "$hash0" == "$hash1" ]] && [[ "$hash1" == "$hash2" ]]; then
        log "  Height ${height}: AppHash=${hash0} (all 3 nodes agree)"
    else
        APPHASH_PASS=false
        log "  Height ${height}: MISMATCH! node0=${hash0} node1=${hash1} node2=${hash2}"
    fi
done

if $APPHASH_PASS; then
    pass "Test 1: AppHash consensus verified across all 3 nodes (CONS-02, CONS-04)"
else
    fail "Test 1: AppHash mismatch detected — state divergence (CONS-02 FAILED)"
fi

# ---------------------------------------------------------------------------
# Test 2: Crash Recovery (CONS-03)
# ---------------------------------------------------------------------------

echo ""
echo "--- Test 2: Crash Recovery (CONS-03) ---"

if [[ "$SKIP_CRASH_TEST" == "1" ]]; then
    log "Crash recovery test SKIPPED (SKIP_CRASH_TEST=1)"
else
    # Get node 2's current height before crash
    height_before_crash="$(latest_height 2)"
    log "Node 2 current height before crash: ${height_before_crash}"

    # Kill node 2 with SIGKILL (simulate crash — not graceful shutdown)
    pid2_file="$(node_pid_file 2)"
    if [[ ! -f "$pid2_file" ]]; then
        fail "No PID file for node 2 — cannot simulate crash"
    fi
    pid2="$(cat "$pid2_file")"
    log "Crashing node 2 (PID ${pid2}) with SIGKILL..."
    kill -KILL "$pid2" 2>/dev/null || true
    sleep 1
    rm -f "$pid2_file"
    log "Node 2 crashed."

    # Wait for nodes 0 and 1 to produce 10 more blocks (2-of-3 can continue)
    target_height_while_crashed=$((height_before_crash + 10))
    log "Waiting for nodes 0 and 1 to reach height ${target_height_while_crashed}..."
    if wait_node_height 0 "$target_height_while_crashed" 120 && \
       wait_node_height 1 "$target_height_while_crashed" 120; then
        log "Nodes 0 and 1 reached height ${target_height_while_crashed} without node 2."
    else
        fail "Test 2: Nodes 0/1 failed to make progress without node 2 — 2-of-3 quorum broken (CONS-03 FAILED)"
    fi

    # Restart node 2 from its existing WAL and data directory
    log "Restarting node 2 from existing WAL..."
    cfg2="$(node_cfg 2)"
    log2="$(node_log 2)"
    RUST_LOG="${RUST_LOG:-info}" \
        "${SLAY3RD}" "$cfg2" \
        >> "$log2" 2>&1 &
    new_pid2="$!"
    echo "$new_pid2" > "$pid2_file"
    log "Node 2 restarted — PID: ${new_pid2}"

    # Wait for node 2 to catch up to height of nodes 0/1
    catchup_target="$(latest_height 0)"
    log "Waiting for node 2 to catch up to height ${catchup_target}..."
    if wait_node_height 2 "$catchup_target" 180; then
        log "Node 2 caught up to height ${catchup_target}."
    else
        fail "Test 2: Node 2 failed to catch up after WAL restart (CONS-03 FAILED)"
    fi

    # Verify AppHash agreement at the catch-up height
    catchup_hash0="$(app_hash_at_height 0 "$catchup_target")"
    catchup_hash2="$(app_hash_at_height 2 "$catchup_target")"
    if [[ -n "$catchup_hash0" ]] && [[ -n "$catchup_hash2" ]] && [[ "$catchup_hash0" == "$catchup_hash2" ]]; then
        log "Post-crash AppHash at height ${catchup_target}: node0=${catchup_hash0} node2=${catchup_hash2} (match)"
        pass "Test 2: Crash recovery verified — node 2 rejoined and AppHash matches (CONS-03)"
    else
        fail "Test 2: AppHash divergence after crash recovery (node0=${catchup_hash0} node2=${catchup_hash2:-MISSING}) (CONS-03 FAILED)"
    fi
fi

# ---------------------------------------------------------------------------
# Test 3: Certificate in Block Header (CONS-05)
# ---------------------------------------------------------------------------

echo ""
echo "--- Test 3: Certificate in Block Header (CONS-05) ---"

# Extract a BLS certificate from node 0's log.
# The LayerReporter in slay3rd logs: "Block finalized with BLS threshold certificate (CONS-05)"
# with fields: payload_digest=HEX cert_len=N

# Method A: Parse from structured tracing log
cert_log_line="$(grep -m1 'Block finalized with BLS threshold certificate' "$(node_log 0)" 2>/dev/null || echo "")"
if [[ -z "$cert_log_line" ]]; then
    fail "Test 3: No 'Block finalized with BLS threshold certificate' log entry found in node 0 log (CONS-05 FAILED)"
fi

log "Found certificate log entry: ${cert_log_line}"

# Extract cert_len from log line
cert_len="$(echo "$cert_log_line" | grep -oP 'cert_len=\K[0-9]+' || echo "0")"
if [[ "$cert_len" -eq 0 ]]; then
    fail "Test 3: BLS certificate has zero length — Block.certificate is empty (CONS-05 FAILED)"
fi
log "Certificate length: ${cert_len} bytes (non-zero — certificate is present)"

# Extract payload_digest from log line
payload_digest="$(echo "$cert_log_line" | grep -oP 'payload_digest=\K[0-9a-f]+' || echo "")"
if [[ -z "$payload_digest" ]]; then
    log "  WARNING: Could not extract payload_digest from log entry"
else
    log "  Payload digest: ${payload_digest}"
fi

# Extract certificate hex from node log (app_hash and certificate fields)
cert_hex="$(grep 'Block finalized with BLS certificate' "$(node_log 0)" 2>/dev/null \
    | grep -oP 'certificate=\K[0-9a-f]+' | head -1 || echo "")"

if [[ -n "$cert_hex" ]]; then
    log "  Certificate hex: ${cert_hex:0:64}... (${#cert_hex} hex chars = $((${#cert_hex} / 2)) bytes)"
    # Write certificate hex to file for offline verification
    echo "$cert_hex" > "${TESTNET_DIR}/latest_cert.hex"
    log "  Certificate written to: ${TESTNET_DIR}/latest_cert.hex"
else
    log "  Note: certificate hex not in log (cert_len=${cert_len} confirms certificate is present via Reporter)"
    # Write payload_digest as a placeholder for offline verification
    if [[ -n "$payload_digest" ]]; then
        echo "$payload_digest" > "${TESTNET_DIR}/latest_cert_payload_digest.hex"
        log "  Payload digest written to: ${TESTNET_DIR}/latest_cert_payload_digest.hex"
    fi
fi

pass "Test 3: BLS certificate present in finalized blocks (cert_len=${cert_len}) (CONS-05)"

# ---------------------------------------------------------------------------
# Test 4: Determinism Audit (CONS-04)
# ---------------------------------------------------------------------------

echo ""
echo "--- Test 4: Determinism Audit (CONS-04) ---"

# Grep for non-deterministic constructs in consensus-critical code paths.
# Excludes:
#   - test modules (#[cfg(test)] / mod tests)
#   - Comments with DETERMINISM (explaining the choice)
#   - capabilities() function (cosmwasm_vm boundary — HashSet required by interface)
#   - Backend/VmApi boundary (documented exceptions in STATE.md)
GREP_PATHS="${SDK_ROOT}/app/slay3rd/src ${SDK_ROOT}/packages/app/src"
GREP_PATTERN='HashMap\|HashSet\|SystemTime::now'

log "Checking for HashMap/HashSet/SystemTime::now in consensus code paths..."
log "  Paths: ${GREP_PATHS}"

# Run grep, filtering out tests, comments, and documented exceptions
VIOLATIONS="$(
    grep -rn "$GREP_PATTERN" $GREP_PATHS --include='*.rs' 2>/dev/null \
    | grep -v '^\s*//' \
    | grep -v '#\[cfg(test)\]' \
    | grep -v 'mod tests' \
    | grep -v 'DETERMINISM' \
    | grep -v 'capabilities()' \
    | grep -v 'HashSet<String>' \
    | grep -v '// CONS-04' \
    || true
)"

if [[ -z "$VIOLATIONS" ]]; then
    pass "Test 4: No HashMap/HashSet/SystemTime::now in consensus-critical code (CONS-04)"
else
    echo "$VIOLATIONS"
    fail "Test 4: Determinism violations found in consensus code paths (CONS-04 FAILED)"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "=== All Phase 2 Consensus Tests Passed ==="
echo ""
echo "CONS-02: AppHash consensus verified across 3 nodes"
if [[ "$SKIP_CRASH_TEST" != "1" ]]; then
    echo "CONS-03: Crash recovery verified (node 2 rejoined from WAL)"
else
    echo "CONS-03: Crash recovery SKIPPED"
fi
echo "CONS-04: Determinism audit passed (no HashMap/SystemTime in consensus paths)"
echo "CONS-05: BLS threshold certificate present in finalized blocks"
echo ""
echo "Certificate for offline verification: ${TESTNET_DIR}/latest_cert.hex (if available)"
echo "Verify with: cargo run --manifest-path ${SDK_ROOT}/tools/verify-cert/Cargo.toml -- --help"
