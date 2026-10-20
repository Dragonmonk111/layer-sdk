#!/usr/bin/env bash
set -eo pipefail

target="riscv32imac-unknown-none-elf"
image="ghcr.io/commonwarexyz/monorepo/rust-riscv32imac-cross@sha256:652f5ff21c943935bc1caf7cf0c65b38127381c66b423f70f86dc7785d93ce85"
base_rustflags="${RUSTFLAGS:-}"

build_cmd=(docker run \
  --rm \
  -v `pwd`:/workdir \
  -w="/workdir" \
  "$image" cargo +nightly build -Zbuild-std=core,alloc --no-default-features --target "$target" --release)
pretty_cmd="${build_cmd[*]}"
if [ -n "$CI" ]; then
  echo "::group::${pretty_cmd}"
else
  printf "\n%s:\n  %s\n" "$pretty_cmd"
fi

RUSTFLAGS="${base_rustflags} -D warnings" "${build_cmd[@]}"
du -h "target/${target}/release/libsriracha_mayo.rlib"

if [ -n "$CI" ]; then
  echo "::endgroup::"
fi
