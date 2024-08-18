#!/bin/bash
set -eux

# This generates grpc-gateway bindings in the gateway directory 

IMAGE="bufbuild/buf:1.23.1"

SUDO="sudo"
if groups | grep -q docker; then
  SUDO=""
fi

$SUDO docker run --rm -w /buf/proto -v $(pwd)/proto:/buf/proto -v $(pwd)/gateway:/buf/gateway "$IMAGE" ls-files
$SUDO docker run --rm -w /buf/proto -v $(pwd)/proto:/buf/proto -v $(pwd)/gateway:/buf/gateway "$IMAGE" generate

# change it back if we ran as root before
if [ -n "$SUDO" ]; then
  WHOAMI=$(whoami)
  sudo chown -R $WHOAMI:$WHOAMI ./gateway
fi
