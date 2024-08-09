# State Sync Testing

How to test statesync.

The approach is based around some print statement in the TestApp that will be removed by the end of the PR, so this is temporary


## Memstore
 
The following will get some output from basic run using MemoryStore, which can help test the actual output we get around raw key -> parsed key.

```bash
cd packages/app
cargo test --tests testing::bank_cw20::contracts_send_receive_cw20_as_native -- --nocapture
```

## Rocks DB

In order to test this works with the real database, we can try the following:

```bash
cd packages/app
cargo test --tests transaction_workflow_rocksdb --features rocksdb -- --nocapture
```

## Future Notes

(State breaking change)
AccountId should encode nicer in JSON (at least hex rather than [84,188,124,191,71,229,153,47,39,153,73,21,75,241,30,202,240,242,177,9], maybe slayer1... address)
- load both [177, 118,...] format as well as stringified AccountId
