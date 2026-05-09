# Unified Artifact: One WASM, Two Contexts

## The Idea

A single `.wasm` component exports two interfaces:
- **`contract::`** — stateful entry points (instantiate, execute, query, migrate). Called by the Layer node when processing transactions.
- **`component::`** — off-chain computation (run). Called by WAVS operators when processing triggers.

Both share types, code, and state definitions. The host provides different capabilities depending on context.

## Why This Works

WASI components use WIT (WebAssembly Interface Types) to declare imports and exports. A component can export multiple interfaces and import multiple capabilities. The host decides:
1. Which exported interface to call
2. Which imported capabilities to provide

This is capability-based security by construction. The component can't do HTTP in contract context because the host simply doesn't provide the `wasi:http` import. It can't write state in component context because the host provides a read-only storage implementation.

## The WIT World

```wit
package layer:ewasm@0.1.0;

/// Storage — host-provided, context-dependent
interface storage {
    /// Read a value by key. Available in both contexts.
    get: func(key: list<u8>) -> option<list<u8>>;

    /// Write a value. Only available in contract context.
    /// In component context, this traps.
    set: func(key: list<u8>, value: list<u8>);

    /// Delete a key. Only available in contract context.
    remove: func(key: list<u8>);

    /// Iterate over a key prefix. Available in both contexts.
    scan: func(prefix: list<u8>, limit: u32) -> list<tuple<list<u8>, list<u8>>>;
}

/// Crypto — host-provided, available in both contexts
interface crypto {
    keccak256: func(data: list<u8>) -> list<u8>;     // 32 bytes
    ecrecover: func(hash: list<u8>, sig: list<u8>) -> result<list<u8>, string>;  // 20 bytes
}

/// HTTP — host-provided, ONLY in component context
/// In contract context, this import is not satisfied (compile-time absent)
interface http {
    // re-export wasi:http/outgoing-handler
}

/// Contract execution context
interface contract {
    record env {
        block-height: u64,
        block-timestamp: u64,
        chain-id: u64,
        contract-address: list<u8>,     // 20 bytes
    }

    record message-info {
        sender: list<u8>,               // 20 bytes
        funds: list<coin>,
    }

    record coin {
        denom: string,
        amount: list<u8>,               // U256 as 32 bytes big-endian
    }

    record response {
        messages: list<sub-msg>,
        events: list<event>,
        data: option<list<u8>>,
    }

    record sub-msg {
        id: u64,
        msg: list<u8>,                  // ABI-encoded WasmMsg or BankMsg
        reply-on: reply-on,
    }

    enum reply-on { success, error, always, never }

    record event {
        name: string,
        topics: list<list<u8>>,         // indexed fields as 32-byte topics
        data: list<u8>,                 // ABI-encoded non-indexed fields
    }

    /// Entry points — Layer node calls these
    instantiate: func(env: env, info: message-info, msg: list<u8>) -> result<response, string>;
    execute: func(env: env, info: message-info, msg: list<u8>) -> result<response, string>;
    query: func(env: env, msg: list<u8>) -> result<list<u8>, string>;
    migrate: func(env: env, info: message-info, msg: list<u8>) -> result<response, string>;
}

/// Component execution — WAVS operators call this
interface component {
    /// Trigger data from WAVS (EVM event, cron, raw, etc.)
    /// Same as existing WAVS trigger types
    use wavs:types/events.{trigger-action, wasm-response};

    /// The off-chain computation entry point
    run: func(action: trigger-action) -> result<list<wasm-response>, string>;
}

/// The unified world — one component exports both
world ewasm {
    // Host capabilities (context-dependent)
    import storage;
    import crypto;
    import wasi:http/outgoing-handler@0.2.3;  // only satisfied in component context
    import wasi:logging/logging;

    // Component exports both interfaces
    export contract;
    export component;
}
```

## What the Rust Code Looks Like

```rust
use layer_ewasm::*;

// Shared state definitions — both sides see these types,
// both sides can read, only contract side can write
const PRICES: Map<String, U256> = Map::new("prices");
const SYMBOLS: Item<Vec<String>> = Item::new("symbols");
const OWNER: Item<Address> = Item::new("owner");
const UPDATE_INTERVAL: Item<u64> = Item::new("interval");

// Shared types — used by both contract and component
#[derive(AbiEncode, AbiDecode)]
pub struct PriceUpdate {
    pub symbol: String,
    pub price: U256,
    pub timestamp: u64,
}

// --- ABI ---

sol! {
    struct InitMsg {
        uint64 update_interval;
        string[] symbols;
    }

    function addSymbol(string symbol);
    function removeSymbol(string symbol);

    function getPrice(string symbol) returns (uint256 price);
    function getSymbols() returns (string[] symbols);

    event PriceUpdated(string indexed symbol, uint256 price, uint64 timestamp);
}

// === CONTRACT SIDE ===
// Layer node calls these when processing transactions.
// Has: storage (read+write), crypto
// Does NOT have: http, filesystem

#[ewasm::contract]
impl PriceOracle {
    pub fn instantiate(ctx: &mut Ctx, info: MessageInfo, msg: InitMsg) -> Result<Response> {
        OWNER.save(ctx, &info.sender)?;
        UPDATE_INTERVAL.save(ctx, &msg.update_interval)?;
        SYMBOLS.save(ctx, &msg.symbols)?;
        Ok(Response::new())
    }

    pub fn execute(ctx: &mut Ctx, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
        match msg {
            ExecuteMsg::addSymbol(msg) => {
                ensure!(info.sender == OWNER.load(ctx)?, EwasmError::Unauthorized);
                let mut symbols = SYMBOLS.load(ctx)?;
                symbols.push(msg.symbol);
                SYMBOLS.save(ctx, &symbols)?;
                Ok(Response::new())
            }
            ExecuteMsg::removeSymbol(msg) => {
                ensure!(info.sender == OWNER.load(ctx)?, EwasmError::Unauthorized);
                let mut symbols = SYMBOLS.load(ctx)?;
                symbols.retain(|s| s != &msg.symbol);
                SYMBOLS.save(ctx, &symbols)?;
                Ok(Response::new())
            }
        }
    }

    /// Called by the framework when WAVS operators reach quorum
    /// on a component::run() result. The `results` are the
    /// ABI-decoded output from the component side.
    #[wavs_callback]
    pub fn on_prices(ctx: &mut Ctx, results: Vec<PriceUpdate>) -> Result<Response> {
        let mut resp = Response::new();
        for update in results {
            PRICES.save(ctx, &update.symbol, &update.price)?;
            resp = resp.add_event(PriceUpdated {
                symbol: update.symbol,
                price: update.price,
                timestamp: update.timestamp,
            });
        }
        Ok(resp)
    }

    pub fn query(ctx: &Ctx, msg: QueryMsg) -> Result<Bytes> {
        match msg {
            QueryMsg::getPrice(msg) => {
                let price = PRICES.load(ctx, &msg.symbol)?;
                Ok(price.abi_encode().into())
            }
            QueryMsg::getSymbols(_) => {
                let symbols = SYMBOLS.load(ctx)?;
                Ok(symbols.abi_encode().into())
            }
        }
    }
}

// === COMPONENT SIDE ===
// WAVS operators call this on trigger.
// Has: storage (READ-ONLY snapshot), crypto, http, logging
// Does NOT have: storage writes, cross-contract calls

#[ewasm::component]
impl PriceOracle {
    #[trigger(cron = "60s")]
    pub fn fetch_prices(ctx: &ComponentCtx) -> Result<Vec<u8>> {
        // Read contract state (read-only snapshot from Layer)
        let symbols = SYMBOLS.load(ctx)?;

        let mut updates = Vec::new();
        for symbol in &symbols {
            // HTTP is available here — not deterministic, that's fine
            let resp: PriceResponse = ctx.http_get_json(
                &format!("https://api.coingecko.com/api/v3/simple/price?ids={symbol}&vs_currencies=usd")
            )?;

            updates.push(PriceUpdate {
                symbol: symbol.clone(),
                price: U256::from(resp.usd * 1_000_000), // 6 decimal fixed point
                timestamp: ctx.now(),
            });

            ctx.log(LogLevel::Info, &format!("Fetched {symbol}: ${}", resp.usd));
        }

        // Output → WAVS operators sign this → quorum → on_prices() callback
        Ok(updates.abi_encode())
    }
}
```

## How Deployment Works

```
$ layer deploy price_oracle.wasm

1. Upload single .wasm component to Layer
2. Layer inspects WIT exports:
   - Has `contract::` interface → register as a stateful contract
   - Has `component::` interface → register as a WAVS service
3. Instantiate contract (call contract::instantiate)
4. Register WAVS service pointing to this contract
   - Trigger config from #[trigger(...)] metadata
   - Callback target = this contract's address
   - Component hash = hash of this .wasm
5. Return: contract address + service ID (same identity)
```

One deploy command. One address. One thing.

## How Execution Works

### Contract calls (transactions)

```
User tx → Layer node
  → load component from cache
  → provide: storage(R+W), crypto
  → do NOT provide: http
  → call contract::execute(env, info, calldata)
  → apply state changes
  → emit events
  → process sub-messages
```

### Component runs (WAVS triggers)

```
Trigger fires → WAVS operator
  → load component (same .wasm)
  → provide: storage(read-only snapshot), crypto, http
  → do NOT provide: storage writes
  → call component::run(trigger_action)
  → operator signs result
  → submit to Layer via contract::execute (wavs_callback)
  → quorum reached → callback executes with R+W storage
```

### State flow

```
                    ┌─────────────────┐
                    │   Layer State    │
                    │   (RocksDB)      │
                    └────┬───────┬─────┘
                         │       │
                    write│       │read-only snapshot
                         │       │
              ┌──────────┴──┐ ┌──┴───────────┐
              │ contract::  │ │ component::   │
              │ execute()   │ │ run()         │
              │ on Layer    │ │ on WAVS ops   │
              │ node        │ │               │
              └─────────────┘ └───────┬───────┘
                                      │
                              signed result
                                      │
                              ┌───────┴───────┐
                              │ quorum check  │
                              │ + callback    │
                              └───────┬───────┘
                                      │
                              contract::execute
                              (wavs_callback)
                              → writes state
```

## What Layer's VM Becomes

Layer's VM is no longer a fork of cosmwasm_vm. It's:

1. **Wasmtime** — same engine WAVS uses for components
2. **Component model** — WIT-based interfaces, not raw WASM imports/exports
3. **Capability-based host** — provide different imports for different contexts
4. **Shared component cache** — same binary serves both contract and component execution

The actor model (isolated storage, authenticated caller, cross-contract calls, query isolation) is implemented as host behavior, not as VM infrastructure. The host:
- Assigns storage namespaces per contract address
- Injects `message-info.sender` from the transaction signer
- Dispatches cross-contract calls by loading the target component and calling its `contract::execute`
- Runs queries against a read-only storage snapshot

This is simpler than cosmwasm_vm because the component model handles serialization boundaries, capability checking, and instance isolation. WIT enforces the interface contract at compile time.

## The Storage Snapshot Problem

The component side needs to read contract state, but it runs on WAVS operators (not the Layer node). How does it get state?

Options:

### A: State proof at trigger time
When a trigger fires, the Layer node provides a Merkle proof of the relevant state. The WAVS operator verifies the proof and provides the state to the component via read-only storage imports.

**Pro:** Trustless. Operator can't lie about state.
**Con:** Component must declare which keys it reads upfront (or the proof is the entire state tree).

### B: Query the Layer node
The component's storage import is backed by RPC calls to a Layer node. The operator fetches state on-demand as the component reads keys.

**Pro:** Simple. No upfront key declaration.
**Con:** Trusts the RPC node. Adds latency per storage read.

### C: State snapshot in trigger data
The trigger includes a serialized snapshot of the contract's storage namespace. The component reads from this in-memory snapshot.

**Pro:** Single round-trip. Component is self-contained during execution.
**Con:** Expensive for contracts with large state. Wasteful if component only reads a few keys.

### D: Hybrid — query with commitment
Operator queries a Layer node for state, but the trigger includes the state root. After execution, the signed result includes the state root it was computed against. The Layer node verifies the result was computed against the correct state.

**Pro:** Simple reads + integrity verification.
**Con:** Adds a state root to the submission protocol.

**Recommendation:** Start with B (query), add D (commitment) when integrity matters. A (proofs) is the endgame for full trustlessness but can come later.

## What This Means for the Layer SDK

The existing cosmwasm_vm is replaced by a wasmtime-based host that:
1. Loads WASI components (not raw WASM modules)
2. Provides the `layer:ewasm` WIT world
3. Implements `storage`, `crypto` as host functions
4. Dispatches to `contract::*` or `component::*` based on context
5. Manages the actor model (namespacing, auth, cross-contract) at the host level

The `layer_ewasm` Rust crate provides:
1. `#[ewasm::contract]` and `#[ewasm::component]` proc macros
2. `Item<T>`, `Map<K, V>` storage wrappers (same API as cw-storage-plus)
3. Re-exported alloy types (`Address`, `U256`, `sol!()`, etc.)
4. ABI encode/decode for entry points
5. `Ctx` / `ComponentCtx` types that wrap the WIT imports

The contract author writes one Rust crate, runs `cargo component build`, and gets one `.wasm` file that works in both contexts.
