#!/bin/bash
set -eux

docker build . -f docker/Dockerfile.gateway -t ghcr.io/lay3rlabs/gateway:latest
docker build . -f docker/Dockerfile.slay3rd -t ghcr.io/lay3rlabs/slay3rd:latest
docker build . -f docker/Dockerfile.faucet -t ghcr.io/lay3rlabs/faucet:latest
