#!/bin/bash
set -eux

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

$SUDO docker build . -f docker/Dockerfile.gateway -t ghcr.io/lay3rlabs/gateway:latest
$SUDO docker build . -f docker/Dockerfile.slay3rd -t ghcr.io/lay3rlabs/slay3rd:latest
$SUDO docker build . -f docker/Dockerfile.faucet -t ghcr.io/lay3rlabs/faucet:latest
