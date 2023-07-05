#!/bin/bash

set -eux

VERSION=1.19
IMAGE="ghcr.io/grpc-ecosystem/grpc-gateway/build-env:${VERSION}"

IMAGE="bufbuild/buf:1.23.1"
docker run --rm -w /buf -v $(pwd):/buf "$IMAGE" generate

docker run --rm -w /buf/proto -v $(pwd)/proto:/buf/proto -v $(pwd)/gateway:/buf/gateway "$IMAGE" generate

# docker pull "$IMAGE"
# docker run --rm -it "$IMAGE"

