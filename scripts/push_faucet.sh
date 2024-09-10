#!/bin/bash
set -eux

# You need a personal access token (classic) from github.
# https://github.com/settings/tokens
# docker login -u USERNAME --password-stdin ghcr.io

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

for img in faucet; do
  $SUDO docker push ghcr.io/lay3rlabs/${img}:latest
done
