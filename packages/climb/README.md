# CLIent for Multiple Blockchains

Climb is a Rust library for interacting with Lay3r and Cosmos.

You can think of it as the Rust alternative to CosmJS (kinda like CosmRS, but, does more out of the box).

## Cargo Docs

The easiest way to get a feel for the library is to check the cargo docs.
As of right now, this isn't published anywhere, so just run `cargo doc --open`

## Prelude

Most of the types are re-exported in the prelude and can be used via the line-liner:

```rust
use layer_climb::prelude::*;
```

## SigningClient

[source code](./src/signing.rs#L20)

A SigningClient needs only two things, a ChainConfig and a TxSigner:

```rust
use layer_climb::prelude::*;

SigningClient::new(chain_config, signer).await
```

The `SigningClient` is cheap to clone and also fairly cheap to create.

#### ChainConfig

[source code](./src/config.rs#L8)

This is a serde-friendly data struct and is typically loaded from disk. See the [example in climb-cli](../../tools/climb-cli/config.json)

#### TxSigner

[source code](./src/transaction.rs#L79)

This is a trait with only two required functions:

```rust
fn sign(&self, doc: &SignDoc) -> Result<Vec<u8>>;
fn public_key(&self) -> PublicKey;
```


For convenience, it can be created from a mnemonic with the provided [KeySigner](./src/signing/key.rs#L16) like:

```rust
use layer_climb::prelude::*;

// None here means "Cosmos derivation path"
let key_signer = KeySigner::new_mnemonic_str(my_mnemonic_string, None)?;
```

This plays nicely with the `bip39` and `rand` Rust crates, so you can easily generate a random mnemonic and pass it to `new_mnemonic_iter`:

```rust
use layer_climb::prelude::*;
use bip39::Mnemonic;
use rand::Rng;

let mut rng = rand::thread_rng();
let entropy: [u8; 32] = rng.gen();
let mnemonic = Mnemonic::from_entropy(&entropy)?;

let signer = KeySigner::new_mnemonic_iter(mnemonic.word_iter(), None)?;
```

In fact, that's exactly how the `generate-wallet` command in [climb-cli](../../tools/climb-cli/) works!

## QueryClient

[source code](./src/querier.rs#L38) 

If you have a SigningClient, then a QueryClient is created for you automatically as `signing_client.querier` and you have the wallet address in `signing_client.addr`

However, often you want to make queries against other addresses for which you don't have the Signing Key

All you need for this is the `ChainConfig`:

```rust
QueryClient::new(chain_config).await
```

The `QueryClient` is cheap to clone and also cheap to create (it uses a cache to re-use a global reqwest client as well as one grpc channel or client per-endpont).

The QueryClient struct is slightly different for wasm32 targets, but this is all dealt with as an abstraction, methods are the same everywhere.

## Addresses

[source code](./src/address.rs#L28)

One difference compared to other clients is that we require knowing the address type. This paves the way for supporting Ethereum-style address strings throughout the client. You can construct an address manually via methods like `new_cosmos()`, but it's more convenient to create it via a method on `ChainConfig`:

```rust
let addr = chain_config.parse_address("address string")?;
```

A similar method exists to derive it from a public key:

```rust
let addr = chain_config.address_from_pub_key(signer.public_key())?;
```

The `Display` implementation for `Address` is a plain string as would typically be expected for display purposes (events, block explorers, etc.)

## Transactions

Generally speaking, you just call a method on the `SigningClient`. For example, here's how to transfer funds:

```rust
use layer_climb::prelude::*;

let amount:u128 = 1_000_000;
let recpient_addr:Address = chain_config.parse_address("address string")?; // see `Addresses` above

// use chain's native gas denom
signing_client.transfer(None, amount, recipient_addr, None).await?;
// some other denom
signing_client.transfer(Some("uusdc"), amount, recipient_addr, None).await?;
```

The last `None` is typical for all transaction methods. It takes a `TxBuilder` which allows configuring per-transaction settings like the gas fee, simulation multiplier, and many more.

[source code](./src/transaction.rs#L29)

Technically, you don't even need a `SigningClient` for transactions, a `TxSigner` + `TxBuilder` + `QueryClient` is enough, but this is unwieldy. When you want to change transaction defaults, it's more convenient to get a `TxBuilder` from the `SigningClient`, and pass that as a parameter to the method (it will automatically pass the `TxSigner` along):


```rust
let tx_builder = signing_client.tx_builder();
tx_builder.set_gas_simulate_multiplier(2.0);
signing_client.transfer(None, amount, recipient_addr, Some(tx_builder)).await?;
```

## Requests / Responses

Internally, Query methods turn the arguments into a struct which implements a QueryRequest trait.

[source code](./src/querier.rs#L52)

The exact implementation here is likely to change, but it's a way to support generic middleware over all requests so we can do things like retry requests on failure, switch from grpc to rpc on any given request (not yet supported), etc. More details on this below.

As an example, calling the [contract_code_info](./src/querier/contract.rs#L32) method on `QueryClient` creates an internal [ContractCodeInfoReq](./src/querier/contract.rs#L113) struct and the actual query is implemented on that struct's [request](./src/querier/contract.rs#L120) method.

This is the pattern for all queries.

## Messages

Transactions work in a similar way, however instead of creating an internal Request type, each method calls an internal helper to create some message, and then broadcasts the message with a TxBuilder.

This allows for calling those message-creating methods separately, and brodcasting them together in one transaction.

The [TxBuilder broadcast method](./src/transaction.rs#L203) takes an iterator of these messages, which must be converted into a protobuf `Any`.

## Events

As a convenience helper to filter and search events, consider using `CosmosTxEvents`. It has `From` impls for various event sources like `TxResponse`, `Vec<Event>`, etc.

[source code](./src/events.rs#L182)

It's a nearly zero-cost abstraction (just dynamic dispatch). Internally, it has variants with references, and so if you pass a reference source there are no allocations.

This is especially helpful for CosmWasm events, so you don't need to worry about the `wasm-` prefix. 

Here's an example of extracting the code id from a contract upload tx:

```rust
let code_id: u64 = CosmosTxEvents::from(&tx_resp)
    .attr_first("store_code", "code_id")?
    .value()
    .parse()?;
```


## Middleware

_the exact architecture here is likely to change_

The QueryClient supports middleware for:

* mapping requests
* mapping responses
* running request -> response

By default it runs a "runner" middleware to retry failing requests up to 3 times with a 100ms delay and exponential backoff.

The TxBuilder supports middleware for:

* mapping TxBody (containing all the messages)
* mapping TxResponse

By default, neither of these are set to anything.

_while the middleware implementations are internally trait-based, attempting to make the middleware field on QueryClient and TxBuilder trait-based, so that third-party middleware is supported, led to some problems with the futures becoming !Send_

## Logging

_this is very likely to change_

As of right now, some methods like IBC handlers take a function to log strings, and there are also logging middleware implementations.

The reason for this was to differentiate between developer logs and user-facing logs and due to the origins of this crate as it was embedded in an application, as opposed to the library it is now.

This will likely move to `tracing` and also decorate the library with tracing instrumention everywhere.

## Errors

_this is very likely to change_

As of right now, errors are merely emitted as `anyhow` strings. While convenient for quick development, it makes error recovery nearly impossible. Most likely this will move to `thiserror`.

## IBC

There are convenient methods for client, connection, and channel handshakes

[source code](./src/signing/ibc/handshake.rs)

With the handshake completed, we can create a fully-functioning IBC relayer:

[source code](./src/signing/ibc/relayer.rs)

The `IbcRelayer` type is constructed from a `IbcRelayerBuilder`. This allows for having a cache that can be re-used across instances, while also mutating the cache when starting up so that expired clients can be recreated.

[source code](./src/signing/ibc/relayer/builder.rs)

The ergonomics of this relayer are intended to support the use-case of relaying over preconfigured ports, not necessarily assuming that the ibc clients are maintained by the ecosystem such as ICS-20 on a popular mainnet.

_note: the relayer has been tested to complete packets from one chain to another, but there is currently an unresolved bug with relaying contract responses. It's recommended to use this only for simple testing, maintained relayers that are used at scale like Hermes or IBC-Relayer should be used in production_ 