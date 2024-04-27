#!/bin/bash
set -eux

# docker build . -f docker/Dockerfile.gateway -t pulsar/gateway:latest
# docker build . -f docker/Dockerfile.pulsariumd -t pulsar/pulsariumd:latest

docker build . -f docker/Dockerfile.gateway -t ghcr.io/lay3rlabs/gateway:latest
docker build . -f docker/Dockerfile.pulsariumd -t ghcr.io/lay3rlabs/lay3rd:latest
docker build . -f docker/Dockerfile.faucet -t ghcr.io/lay3rlabs/faucet:latest
