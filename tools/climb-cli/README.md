# Climb CLI

A universal Rust CLI for the layer-sdk chains. It builds on the [`climb`](../../packages/climb/) package, which can compile
to native, in-browser wasm, and soon WASI.

## Running a local node

For all the below commands, you need to make sure you have a localnode running.
Instructions are in the [localnode directory](../../localnode/README.md),
but the shortcut is:

```bash
# From workspace root
./scripts/build_docker.sh
./localnode/reset_volumes.sh
./localnode/run.sh

# do whatever below. when you want to stop it

./localnode/stop.sh
```

If you hit any errors, please check the full README

## Setup

To start, you need to get a unique mnemonic and store it. The easiest way is:

```bash
# from the workspace root
cd js
npm run generate-mnemonic
```

In this directory, create a file called `.env` with `LOCAL_MNEMONIC="<mnemonic provided above>".
Now, check you can view it

```bash
cargo run wallet-show
```

## Getting tokens

The next step is to "tap the faucet" to get some tokens for your new address,
so you can use the CLI more:

```bash
cargo run wallet-show

cargo run tap-faucet

cargo run wallet-show
```

Yeah, you got some tokens now. Let's go do some more stuff...