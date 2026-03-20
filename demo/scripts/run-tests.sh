#!/bin/bash
# demo/scripts/run-tests.sh
#
# Run the consensus unit tests that validate the Layer state machine.
#
# These tests exercise the full certify/finalize_block path in a single process:
#   - genesis() returns a deterministic 32-byte digest
#   - certify() calls finalize_block and advances block height
#   - BLS certificate storage round-trip (CONS-05)
#   - No state mutation during verify() (CONS-02 safety property)

set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

echo "=== Layer Consensus Unit Tests ==="
echo ""

# Consensus node tests (determinism, block production, certify/verify)
echo "--- slay3rd consensus tests ---"
cargo test -p slay3rd --manifest-path "${SDK_ROOT}/Cargo.toml" -- --nocapture 2>&1 | tail -20
echo ""

# App layer tests (certificate storage round-trip)
echo "--- layer-app certificate storage test ---"
cargo test -p layer-app --manifest-path "${SDK_ROOT}/Cargo.toml" \
    -- test_set_and_get_block_certificate --nocapture 2>&1
echo ""

echo "=== All tests passed ==="
