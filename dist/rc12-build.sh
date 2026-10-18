#!/bin/bash
set -e
export PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin
rm -rf /root/rc12-build
git clone -q /mnt/c/cosmos-node/node-data/config/CascadeProjects/windsurf-project/junoclaw-chain /root/rc12-build
cd /root/rc12-build
git checkout -q commonware
git log --oneline -3
cargo build --release -p slay3rd
echo "BUILD_DONE $(ls -la target/release/slay3rd)"
