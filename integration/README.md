# Integration Tests

This is a series of high level integration tests using CosmJS, that run through
the full stack - connecting to Tendermint RPC, then to Pulsariumd via ABCI.

They should be used occasionally as a sanity check for compatibility with CosmJS,
and also as a place to generate test vectors for fixtures for Rust unit tests.

## Running Tests

First, we assume you have installed everything and know [how to run pulsariumd](../app/pulsariumd/README.md).
This is currently designed to be run and debugged manually, and not set up for CI.
This may change in the future.

Using Docker may also make this simpler / more reproducable.

### Start Nodes

Copy over startup files

```shell
rm -rf ~/.pulse-test
cp -r ./etc ~/.pulse-test
# mkdir -p ~/.pulse-test/lmdb-1
```

Run in one terminal:

```shell
pulsariumd --home ~/.pulse-test

# or dev mode
cd ../app/pulsariumd
cargo run -- --home ~/.pulse-test
```

Run in another terminal:


```shell
cometbft start --home ~/.pulse-test --proxy_app tcp://localhost:26658
```

You should see blocks being produced. Now you are ready to run the tests

### Run Tests

Some tests may succeed running multiple times on the same node. Others may fail.
If you have any surprising failures, wipe out `~/.pulse-test` and repeat the above
steps to get a fresh node to test against.

```shell
npm ci
npm run test
```
