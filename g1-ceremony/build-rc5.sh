#!/usr/bin/env bash
set -e
export PATH="$HOME/.cargo/bin:$PATH"
apt-get update -qq
apt-get install -y -qq curl clang libclang-dev g++ pkg-config protobuf-compiler cmake > /dev/null
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh
sh /tmp/rustup.sh -y --default-toolchain stable --profile minimal > /dev/null
export CARGO_TARGET_DIR=/build
cd /src
cargo build --release -p slay3rd --features slay3rd/rocksdb
cp /build/release/slay3rd /src/g1-ceremony/slay3rd-rc5-glibc235
echo "BUILD_DONE"
