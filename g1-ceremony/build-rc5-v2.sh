#!/usr/bin/env bash
set -e
export PATH="$HOME/.cargo/bin:$PATH"
apt-get update -qq
apt-get install -y -qq curl clang libclang-dev g++ pkg-config protobuf-compiler cmake > /dev/null
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh
sh /tmp/rustup.sh -y --default-toolchain 1.97.0 --profile minimal > /dev/null
export PATH="$HOME/.cargo/bin:$PATH"
# wasmer 4.3.7 references __rust_probestack which was removed from Rust stdlib —
# shim it exactly like docker/Dockerfile.slay3rd does.
cc -c -o /tmp/shim.o /src/docker/shim.c
export RUSTFLAGS="-C link-arg=/tmp/shim.o"
export CARGO_TARGET_DIR=/build
cd /src
cargo build --release --bin slay3rd --features rocksdb
cp /build/release/slay3rd /src/g1-ceremony/slay3rd-rc5-glibc235
echo "BUILD_DONE"
