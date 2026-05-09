# Example: Counter

The hello world. Simple state, simple messages.

```rust
use layer_ewasm::*;

// State — typed storage items, same ergonomics as cw-storage-plus
const COUNT: Item<u64> = Item::new("count");
const OWNER: Item<Address> = Item::new("owner");

// Messages — defined with sol!() so they're ABI-encoded on the wire
sol! {
    // Instantiate
    struct InitMsg {
        uint64 count;
    }

    // Execute variants as separate structs, dispatched by selector
    function increment();
    function reset(uint64 count);

    // Query
    function getCount() returns (uint64);

    // Events
    event Incremented(address indexed by, uint64 new_count);
    event Reset(address indexed by, uint64 new_count);
}

#[ewasm_contract]
impl Counter {
    pub fn instantiate(deps: DepsMut, _env: Env, info: MessageInfo, msg: InitMsg) -> Result<Response> {
        COUNT.save(deps.storage, &msg.count)?;
        OWNER.save(deps.storage, &info.sender)?;
        Ok(Response::new())
    }

    pub fn execute(deps: DepsMut, _env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
        match msg {
            ExecuteMsg::increment(_) => {
                let count = COUNT.load(deps.storage)? + 1;
                COUNT.save(deps.storage, &count)?;
                Ok(Response::new()
                    .add_event(Incremented { by: info.sender, new_count: count }))
            }
            ExecuteMsg::reset(msg) => {
                let owner = OWNER.load(deps.storage)?;
                ensure!(info.sender == owner, EwasmError::Unauthorized);
                COUNT.save(deps.storage, &msg.count)?;
                Ok(Response::new()
                    .add_event(Reset { by: info.sender, new_count: msg.count }))
            }
        }
    }

    pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Bytes> {
        match msg {
            QueryMsg::getCount(_) => {
                let count = COUNT.load(deps.storage)?;
                Ok(count.abi_encode().into())
            }
        }
    }
}
```

## What's Happening

- `sol!()` defines messages as Solidity types — they ABI-encode/decode automatically
- `#[ewasm_contract]` proc macro generates WASM entry points and selector-based dispatch (like Solidity's `bytes4(keccak256("increment()"))`)
- Storage uses `Item<T>` / `Map<K, V>` — same as CosmWasm but addresses are `Address` not `Addr`
- Events are Solidity events — `indexed` fields become topics, the rest is ABI-encoded data
- `info.sender` is an `Address` (20 bytes, 0x-prefixed)
