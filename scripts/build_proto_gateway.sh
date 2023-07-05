#!/bin/bash
set -eux

IMAGE="bufbuild/buf:1.23.1"

# This generates grpc-gateway bindings in the gateway directory 

docker run --rm -w /buf/proto -v $(pwd)/proto:/buf/proto -v $(pwd)/gateway:/buf/gateway "$IMAGE" ls-files
docker run --rm -w /buf/proto -v $(pwd)/proto:/buf/proto -v $(pwd)/gateway:/buf/gateway "$IMAGE" generate
