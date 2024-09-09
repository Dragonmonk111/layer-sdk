#!/bin/bash

# Runs elastic search jaeger backend, but doesn't run the faucet

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

cd "$SCRIPT_DIR"
$SUDO docker compose -f docker-compose.yml -f jaeger-elastic-compose.yml up -d
