#!/bin/bash

ARCH=""
case $(uname -m) in
    x86_64) ARCH="" ;;
    arm64)  ARCH="-arm64" ;;
    *) echo "Unknown architecture "; uname -m ; exit 1;;
esac

# compile all files in contracts directory
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="pulsar_contracts_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  "cosmwasm/workspace-optimizer${ARCH}:0.12.13"

# move a copy of the artifacts to the test fixtures
echo "Copying artifacts to packages/app/fixtures"
for art in artifacts/*.wasm; do
    outfile=$(basename "$art" | sed 's/-aarch64//')
    cp "$art" "packages/app/fixtures/$outfile"
done