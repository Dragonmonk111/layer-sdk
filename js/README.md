# Integration Tests

This is a series of high level integration tests using CosmJS, that run through
the full stack - connecting to Tendermint RPC, then to Slay3rd via ABCI.

They should be used occasionally as a sanity check for compatibility with CosmJS,
and also as a place to generate test vectors for fixtures for Rust unit tests.

## Running Tests

First, we assume you have installed everything and know [how to run slay3rd](../app/slay3rd/README.md).
This is currently designed to be run and debugged manually, and not set up for CI.
This may change in the future.

Using Docker may also make this simpler / more reproducable.

### Start Nodes

Copy over startup files

```shell
rm -rf ~/.layer-test
cp -r ./etc ~/.layer-test
# mkdir -p ~/.layer-test/lmdb-1
```

Run in one terminal:

```shell
slay3rd --home ~/.layer-test

# or dev mode
cd ../app/slay3rd
cargo run -- --home ~/.layer-test
```

Run in another terminal:


```shell
cometbft start --home ~/.layer-test --proxy_app tcp://localhost:26658
```

You should see blocks being produced. Now you are ready to run the tests

### Run Tests

Some tests may succeed running multiple times on the same node. Others may fail.
If you have any surprising failures, wipe out `~/.layer-test` and repeat the above
steps to get a fresh node to test against.

```shell
npm ci
npm run test
```
