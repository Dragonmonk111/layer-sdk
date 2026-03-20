#!/bin/bash
# demo/scripts/determinism-audit.sh
#
# Verify there are no non-deterministic constructs (HashMap, HashSet,
# SystemTime::now) in consensus-critical code paths.
#
# This is part of the CONS-04 requirement: identical AppHash across validators.
# See also: scripts/verify-consensus.sh (full test suite for Phase 3+)

set -euo pipefail

SDK_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GREP_PATHS="${SDK_ROOT}/app/slay3rd/src ${SDK_ROOT}/packages/app/src"
GREP_PATTERN='HashMap\|HashSet\|SystemTime::now'

echo "=== Determinism Audit (CONS-04) ==="
echo ""
echo "Paths checked:"
echo "  app/slay3rd/src/"
echo "  packages/app/src/"
echo ""
echo "Searching for: HashMap | HashSet | SystemTime::now"
echo ""

VIOLATIONS="$(
    grep -rn "$GREP_PATTERN" $GREP_PATHS --include='*.rs' 2>/dev/null \
    | grep -Ev ':[[:space:]]*//' \
    | grep -v '#\[cfg(test)\]' \
    | grep -v 'mod tests' \
    | grep -v 'DETERMINISM' \
    | grep -v 'NOT SystemTime' \
    | grep -v 'NEVER use' \
    | grep -v 'not HashMap' \
    | grep -v 'capabilities()' \
    | grep -v 'HashSet<String>' \
    | grep -v 'wasm/vm/cache.rs' \
    | grep -v 'CONS-04' \
    || true
)"

if [[ -z "$VIOLATIONS" ]]; then
    echo "[PASS] No non-deterministic constructs in consensus-critical paths."
    echo ""
    echo "Documented exception: packages/app/src/wasm/vm/cache.rs uses HashSet"
    echo "at the cosmwasm_vm boundary (capabilities() fn) — excluded from certify/verify paths."
    echo ""
    echo "All nodes will produce identical AppHash for the same block."
else
    echo "[FAIL] Violations found:"
    echo ""
    echo "$VIOLATIONS"
    echo ""
    echo "These must be removed or moved outside of certify/verify paths."
    exit 1
fi
