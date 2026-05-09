# Design Decisions

Rationale for key choices in the EWASM contract model.

## `sol!()` for message types

ABI encoding is the wire format. Contracts speak the same language as EVM tooling — Ethereum wallets and tools can construct calls directly without custom serialization. The `alloy_sol_types` crate generates Rust types from Solidity syntax, giving us type-safe ABI encode/decode for free.

Alternative considered: keep JSON (CosmWasm-compatible). Rejected because it adds a serialization boundary that doesn't exist in the EVM world, and makes cross-chain tooling harder.

## Function selector dispatch

`#[ewasm_contract]` generates `match` on `bytes4` selectors, same as Solidity. The `ExecuteMsg` / `QueryMsg` enums are generated from the `sol!()` block's `function` declarations.

This means any tool that can construct Solidity calldata (ethers.js, viem, cast, wallets) can call EWASM contracts without a custom client library.

## `Item<T>` / `Map<K, V>` storage

CosmWasm got this right. Typed, namespaced, ergonomic. The storage model is the best part of CosmWasm's contract SDK — no reason to change it.

Under the hood, keys are prefixed by namespace (the string passed to `::new()`), same as `cw-storage-plus`. The storage backend is the Layer node's KV store (RocksDB), with each contract getting an isolated prefix.

## `Address` everywhere, no `Addr`

No `Addr`, no `String` addresses, no bech32. `alloy_primitives::Address` is the one address type. It's 20 bytes, checksummed hex with `0x` prefix, and every EVM tool understands it.

This eliminates the CosmWasm pattern of "receive address as `String`, validate with `deps.api.addr_validate()`, store as `Addr`". In EWASM, addresses arrive as `Address` (ABI-decoded from `bytes20`) and are used directly.

## Events as Solidity events

`event Foo(...)` in `sol!()` generates a Rust struct. `Response::add_event()` accepts it. Indexed fields become log topics, non-indexed fields are ABI-encoded data.

This maps directly to EVM event logs. When Layer state is rolled up to Ethereum (via zkVM), these events can be verified against the state root — same format, no translation needed.

## Host functions for crypto

`deps.api.ecrecover`, `deps.api.keccak256` — provided by the host (Layer node), not computed in the WASM sandbox. Same pattern as CosmWasm's `deps.api.secp256k1_verify`.

This is cheaper (native code, not WASM), safer (audited implementations), and keeps the contract code focused on business logic.

## Cross-contract query via ABI

`deps.api.query_contract(addr, abi_bytes)` — the callee is another EWASM contract, query dispatch uses the same selector-based mechanism. The caller constructs calldata with `FooCall { ... }.abi_encode()`, the callee decodes it in its `query` handler.

This is composability without JSON parsing — contracts interoperate through a shared ABI standard.

## `#[ewasm_contract]` proc macro

The proc macro on `impl ContractName { ... }` generates:
1. WASM entry points (exported functions the host calls)
2. Selector-based dispatch for execute and query
3. The `ExecuteMsg` and `QueryMsg` enums (derived from `sol!()` function declarations)
4. Boilerplate for deserializing `Env`, `MessageInfo`, and calldata from host-provided buffers

Contract authors write plain Rust methods. The macro handles the wiring.

## What the proc macro does NOT do

- No automatic storage migration (contracts handle this in `migrate`)
- No automatic access control (contracts check `info.sender` explicitly)
- No magic — the generated code is straightforward dispatch, inspectable via `cargo expand`
