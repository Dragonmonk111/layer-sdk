# EWASM SDK Surface Area

The `layer_ewasm` crate — what contract authors import.

```rust
// layer_ewasm — the contract SDK crate

// Re-export EVM types as first-class citizens
pub use alloy_primitives::{Address, U256, I256, U128, Bytes, FixedBytes, B256};
pub use alloy_sol_types::{sol, SolValue, SolType};

// Storage (same ergonomics as cw-storage-plus, but with EVM key types)
pub use storage::{Item, Map, Prefix};

// Contract context
pub struct Env {
    pub block_height: u64,
    pub block_timestamp: u64,    // unix seconds
    pub chain_id: u64,
    pub contract: Address,       // this contract's address
}

pub struct MessageInfo {
    pub sender: Address,         // tx signer / caller
    pub funds: Vec<Coin>,
}

pub struct Coin {
    pub denom: String,
    pub amount: U256,
}

// Deps — what contracts get to interact with the host
pub struct Deps<'a> {
    pub storage: &'a dyn Storage,
    pub api: &'a dyn Api,
}
pub struct DepsMut<'a> {
    pub storage: &'a mut dyn Storage,
    pub api: &'a dyn Api,
}

// Api provides host functions
pub trait Api {
    fn addr_validate(&self, addr: &str) -> Result<Address, EwasmError>;
    fn keccak256(&self, data: &[u8]) -> B256;
    fn ecrecover(&self, hash: &B256, signature: &[u8; 65]) -> Result<Address, EwasmError>;
    fn query_contract(&self, addr: Address, msg: Vec<u8>) -> Result<Vec<u8>, EwasmError>;
}

// Response — what contracts return
pub struct Response {
    pub messages: Vec<SubMsg>,
    pub events: Vec<Event>,
    pub data: Option<Bytes>,     // ABI-encoded return data
}

// Events are EVM-style logs
pub struct Event {
    pub name: String,
    pub attributes: Vec<(String, String)>,
}

// Cross-contract calls
pub enum WasmMsg {
    Execute { contract: Address, msg: Bytes, funds: Vec<Coin> },
    Instantiate { code_id: u64, msg: Bytes, funds: Vec<Coin>, label: String, salt: Option<B256> },
}

// Bank messages
pub enum BankMsg {
    Send { to: Address, amount: Vec<Coin> },
    Burn { amount: Vec<Coin> },
}
```

## Key Differences from CosmWasm

| Concern | CosmWasm | EWASM |
|---------|----------|-------|
| Address type | `Addr` (bech32 string) | `Address` (20-byte, 0x-prefixed) |
| Message encoding | JSON (`serde_json`) | ABI (`alloy_sol_types`) |
| Message definition | `#[cw_serde] enum` | `sol! { function foo(...) }` |
| Dispatch | JSON tag matching | Function selector (bytes4) |
| Events | `Response::add_attribute` | Solidity `event` structs with indexed topics |
| Crypto | `deps.api.secp256k1_verify` | `deps.api.ecrecover` (returns address) |
| Query return | `to_json_binary(&response)` | `response.abi_encode()` |

## What Stays the Same

- Actor model: `instantiate` / `execute` / `query` / `migrate`
- `Deps` / `DepsMut` for host access
- `Item<T>` / `Map<K, V>` typed storage
- `Response` with sub-messages and events
- Isolated storage namespaces per contract
- Authenticated `info.sender` on every call
- Read-only queries against committed state
