#!/bin/bash
# demo/scripts/keygen-and-inspect.sh
#
# Generate testnet key material and display what each validator gets.
# This is the first step before running any node.

set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${1:-/tmp/layer-demo}"
NUM_VALIDATORS=3

echo "=== Layer Testnet Key Generation ==="
echo ""
echo "Output directory: ${OUTPUT_DIR}"
echo "Validators: ${NUM_VALIDATORS}"
echo ""

cargo run --manifest-path "${SDK_ROOT}/tools/generate-testnet-keys/Cargo.toml" -- \
    --output-dir "${OUTPUT_DIR}" \
    --validators "${NUM_VALIDATORS}" \
    2>&1

echo ""
echo "=== Key Material Summary ==="
echo ""

for i in $(seq 0 $((NUM_VALIDATORS - 1))); do
    KEY_FILE="${OUTPUT_DIR}/validator-${i}/keys.json"
    if [[ ! -f "$KEY_FILE" ]]; then
        echo "  validator-${i}: MISSING (${KEY_FILE})"
        continue
    fi

    # Extract key fields using python3 (available on macOS by default)
    SHARE_INDEX=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['share_index'])")
    THRESHOLD_REQ=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['threshold_required'])")
    THRESHOLD_TOT=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['threshold_total'])")
    ED25519_PUB=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['ed25519_public_hex'][:16] + '...')")
    BLS_PUB=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['bls_public_hex'][:16] + '...')")
    THRESHOLD_PUB=$(python3 -c "import json,sys; d=json.load(open('${KEY_FILE}')); print(d['threshold_public_key_hex'][:16] + '...')")

    echo "  validator-${i}:"
    echo "    share_index:         ${SHARE_INDEX} (1-based)"
    echo "    threshold:           ${THRESHOLD_REQ}-of-${THRESHOLD_TOT}"
    echo "    ed25519_public:      ${ED25519_PUB}"
    echo "    bls_public_share:    ${BLS_PUB}"
    echo "    threshold_pubkey:    ${THRESHOLD_PUB} (same for all validators)"
    echo ""
done

echo "Full key material at: ${OUTPUT_DIR}/validator-*/keys.json"
echo ""
echo "Note: Keys use a fixed seed (ChaCha8Rng(0)) — reproducible but NOT for production."
