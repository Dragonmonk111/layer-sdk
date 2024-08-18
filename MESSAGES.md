# Adding messages

This is a living document, meant to give some pointers on how we add messages and queries to the system.

## Internal Messages

Slay3r uses it's own (unserialized) types to route messages and queries internally to the various modules.
You can find these types here:

* [`slay3r_std::Msg`](./packages/std/src/msg.rs)
* [`slay3r_std::QUery`](./packages/std/src/query.rs)

We only create these types internally, and notably they use [`AccountId`](https://github.com/Lay3rLabs/layer-sdk/blob/main/packages/std/src/account_id.rs), which is a parsed and validated bytes rather than the bech32 string (which this serializes as).

In `slay3r_app::StateMachine`, we route these at 
[StateMachine::process_message](https://github.com/Lay3rLabs/layer-sdk/blob/main/packages/app/src/sm.rs#L131-L157) and 
[StateMachine::query](./packages/app/src/sm.rs#L70-L98) to the proper module.
We define the set of modules at compile-time, rather than dynamic, extensible hooks,
so we can be very strictly types here and do exhaustive matches.

The modules then handle them, and return a strictly typed, deep enums: [`slay3r_std::MsgData`](./std/src/msg.rs#L229-L233) and [`slay3r_std::QueryResponse`](./packages/std/src/query.rs#L76-L83)

Notably you can already write internal tests here without worrying about auth logic, protobuf encodings or any nasty serialization and setup.

## Conversion to CosmWasm

CosmWasm contracts return `CosmosMsg` and call `cosmwasm_std::Query` objects. These are not natively supported
by slay3r (as we want our API to be able to evolve independently from CosmWasm), but there is generally
a relatively simple mapping from these object to the equivalent `slay3r::{Msg,Query}` types and from
those reponses to the CosmWasm responses.

The location of this transition should probably be refactored sometime, but you can find the 
query-related calls in `slay3r_app::wasm::vm::backend`:

* [`cosmwasm_query_to_pulsar`](https://github.com/Lay3rLabs/layer-sdk/blob/main/packages/app/src/wasm/vm/backend.rs#L170-L211)
* [`slay3r_response_to_cosmwasm`](https://github.com/Lay3rLabs/layer-sdk/blob/main/packages/app/src/wasm/vm/backend.rs#L220-L260)

The messages are dispatched in `WasmKeeper::dispatch_response_messages` and the calls are at:

* [`cosmwasm_msg_to_pulsar`](./packages/app/src/wasm/keeper.rs#L846-L923)
* [`encode_cosmwasm_response`](./packages/app/src/wasm/keeper.rs#L927-L988)

## Conversion to SDK Msg

This happens in [`slayer_cosmos`](./packages/cosmos) but is rather complex and will be explained more later.

[This commit](https://github.com/Lay3rLabs/layer-sdk/pull/90/commits/e7c63f0b256e6f36f697094369ebd5fe89cfe607) is a nice example of how we handle
queries. It includes attaching the grpc handler for the query, the abci handler, and adding a new internal query type, which
is added to the wasm keeper. At the least it shows you all the places that need to be updated to add a query full-stack.

**TODO**: example with messages
