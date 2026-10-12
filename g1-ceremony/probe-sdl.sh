#!/bin/bash
# probe-sdl.sh - inspect SDL expose config + embedded node.toml listen port
BASE=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony
SDL=$BASE/_secrets/deploy-akash-seat.yaml
echo "=== SDL top-level structure ==="
grep -nE '^(services:|  [a-zA-Z0-9_-]+:|    (image|env|expose|args|command)|      - (port|as|to|proto|accept)|profiles:|placement:|deployment:)' "$SDL" | head -40
echo
echo "=== expose block ==="
grep -nA12 'expose:' "$SDL" | head -30
echo
echo "=== embedded node.toml listen/dialable ==="
cat /tmp/embedded-node0.toml 2>/dev/null || (grep 'NODE_TOML_B64=' "$SDL" | sed 's/.*NODE_TOML_B64=//' | tr -d '[:space:]' | base64 -d)
