#!/bin/bash
set -eux

# You need a personal access token (classic) from github.
# https://github.com/settings/tokens
# docker login -u USERNAME --password-stdin ghcr.io

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

$SUDO docker push ghcr.io/lay3rlabs/gateway:latest
$SUDO docker push ghcr.io/lay3rlabs/slay3rd:latest
$SUDO docker push ghcr.io/lay3rlabs/faucet:latest