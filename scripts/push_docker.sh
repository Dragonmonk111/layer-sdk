#!/bin/bash
set -eux

# You need a personal access token (classic) from github.
# https://github.com/settings/tokens
# docker login -u USERNAME --password-stdin ghcr.io

docker push ghcr.io/lay3rlabs/gateway:latest
docker push ghcr.io/lay3rlabs/lay3rd:latest