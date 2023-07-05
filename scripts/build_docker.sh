#!/bin/bash
set -eux

docker build . -f docker/Dockerfile.gateway -t pulsar/gateway:latest
docker build . -f docker/Dockerfile.pulsariumd -t pulsar/pulsariumd:latest