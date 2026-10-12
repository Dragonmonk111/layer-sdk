#!/bin/bash
# probe-deploy3.sh - compare SDL-computed version vs on-chain hash; diff embedded vs on-disk node.toml
set -e
BASE=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony
SDL=$BASE/_secrets/deploy-akash-seat.yaml
echo "=== on-chain deployment.hash ==="
echo "UvBWHWSnLclkoLWpLDY5W+k+D107d1xMhPfU1Xgc0NE="
echo
echo "=== version computed by CLI from current SDL (generate-only) ==="
akash tx deployment update "$SDL" --dseq 28973001 \
  --from akash1eehlc3mu8tdkhp7pc4whjkpkw329j9sq6l05dt \
  --generate-only --chain-id akashnet-2 --node https://rpc.akashnet.net:443 2>&1 | head -40
echo
echo "=== extract NODE_TOML_B64 from SDL, compare to on-disk node-0.toml ==="
grep 'NODE_TOML_B64=' "$SDL" | sed 's/.*NODE_TOML_B64=//' | tr -d "[:space:]" | base64 -d > /tmp/embedded-node0.toml 2>/dev/null || true
wc -c /tmp/embedded-node0.toml "$BASE/ceremony-out/validator-0/node-0.toml" 2>/dev/null
diff /tmp/embedded-node0.toml "$BASE/ceremony-out/validator-0/node-0.toml" && echo "IDENTICAL" || echo "^^ DIFFERS"
