#!/bin/bash
# probe-docker.sh - docker daemon, ghcr creds, rc5 binary presence
DOCKER="/mnt/c/Program Files/Docker/Docker/resources/bin/docker"
BIN=/mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain/g1-ceremony/slay3rd-rc5-glibc235
echo "=== binary ==="
ls -la "$BIN" 2>/dev/null || echo "MISSING"
file "$BIN" 2>/dev/null | head -2
echo "=== docker daemon ==="
"$DOCKER" version --format '{{.Server.Version}}' 2>&1 | head -5
echo "=== docker creds (config.json) ==="
for cfg in ~/.docker/config.json "/mnt/c/Users/$USER/.docker/config.json"; do
  [ -f "$cfg" ] && { echo "found: $cfg"; python3 -c "import json,sys; c=json.load(open('$cfg')); print('auths:', list(c.get('auths',{}).keys())); print('credsStore:', c.get('credsStore'))"; }
done
echo "=== try anon pull of rc1 manifest (public check) ==="
"$DOCKER" manifest inspect ghcr.io/dragonmonk111/junoclaw-chain:v0.6.0-rc1 2>&1 | head -8
