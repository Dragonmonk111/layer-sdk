#!/bin/bash

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

cd "$SCRIPT_DIR"
# maybe overkill (if we just did run.sh) but it will ensure we stop everything
$SUDO docker compose -f docker-compose.yml -f jaeger-elastic-compose.yml --profile faucet down
