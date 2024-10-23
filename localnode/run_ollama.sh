#!/bin/bash

SCRIPT_DIR="$(realpath "$(dirname "$0")")"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

cd "$SCRIPT_DIR"
$SUDO docker compose -f docker-compose.ollama.yml --profile faucet --profile wasmatic up -d
