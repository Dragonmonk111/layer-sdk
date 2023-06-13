#!/bin/bash

# Add source of any fixtures here, so we can bump versions if needed
FILES=(
https://github.com/CosmWasm/cosmwasm/releases/download/v1.2.6/hackatom.wasm
https://github.com/CosmWasm/cw-plus/releases/download/v1.0.1/cw20_base.wasm
)

for file in ${FILES[@]}; do
  echo "Downloading $file"
  curl -L -s -O "$file"
done