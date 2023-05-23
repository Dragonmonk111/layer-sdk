# Integration Tests

This is a series of high level integration tests using CosmJS, that run through
the full stack - connecting to Tendermint RPC, then to Pulsariumd via ABCI.

They should be used occasionally as a sanity check for compatibility with CosmJS,
and also as a place to generate test vectors for fixtures for Rust unit tests.
