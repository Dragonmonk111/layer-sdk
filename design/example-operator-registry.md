# Example: Operator Registry

Register operators, stake, slash. The kind of contract WAVS needs on Layer.

```rust
use layer_ewasm::*;

// --- State ---

const OWNER: Item<Address> = Item::new("owner");
const MIN_STAKE: Item<U256> = Item::new("min_stake");
// Map<operator_address, OperatorInfo>
const OPERATORS: Map<Address, OperatorInfo> = Map::new("operators");
// Map<service_id, ServiceConfig>
const SERVICES: Map<U256, ServiceConfig> = Map::new("services");

#[derive(EwasmSerialize)]
pub struct OperatorInfo {
    pub signing_key: FixedBytes<32>,    // BLS or ECDSA pubkey
    pub stake: U256,
    pub registered_at: u64,
    pub slashed: bool,
}

#[derive(EwasmSerialize)]
pub struct ServiceConfig {
    pub component_hash: B256,           // WASM component content hash
    pub quorum_threshold: u64,          // e.g. 67 for 2/3
    pub operator_count: u64,
}

// --- ABI ---

sol! {
    struct InitMsg {
        uint256 min_stake;
    }

    function registerOperator(bytes32 signing_key) payable;
    function deregisterOperator();
    function slash(address operator, bytes proof);
    function registerService(bytes32 component_hash, uint64 quorum_threshold);

    function getOperator(address operator) returns (
        bytes32 signing_key, uint256 stake, uint64 registered_at, bool slashed
    );
    function getService(uint256 service_id) returns (
        bytes32 component_hash, uint64 quorum_threshold, uint64 operator_count
    );
    function isOperator(address operator) returns (bool);

    event OperatorRegistered(address indexed operator, bytes32 signing_key, uint256 stake);
    event OperatorSlashed(address indexed operator, uint256 amount);
    event ServiceRegistered(uint256 indexed service_id, bytes32 component_hash);
}

// --- Contract ---

#[ewasm_contract]
impl OperatorRegistry {
    pub fn instantiate(deps: DepsMut, _env: Env, info: MessageInfo, msg: InitMsg) -> Result<Response> {
        OWNER.save(deps.storage, &info.sender)?;
        MIN_STAKE.save(deps.storage, &msg.min_stake)?;
        Ok(Response::new())
    }

    pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
        match msg {
            ExecuteMsg::registerOperator(msg) => {
                ensure!(!OPERATORS.has(deps.storage, &info.sender), EwasmError::AlreadyRegistered);

                // Check stake from attached funds
                let stake = info.funds.iter()
                    .find(|c| c.denom == "ulayer")
                    .map(|c| c.amount)
                    .unwrap_or(U256::ZERO);
                let min = MIN_STAKE.load(deps.storage)?;
                ensure!(stake >= min, EwasmError::InsufficientStake { required: min, provided: stake });

                OPERATORS.save(deps.storage, &info.sender, &OperatorInfo {
                    signing_key: msg.signing_key,
                    stake,
                    registered_at: env.block_timestamp,
                    slashed: false,
                })?;

                Ok(Response::new()
                    .add_event(OperatorRegistered {
                        operator: info.sender,
                        signing_key: msg.signing_key,
                        stake,
                    }))
            }

            ExecuteMsg::slash(msg) => {
                let owner = OWNER.load(deps.storage)?;
                ensure!(info.sender == owner, EwasmError::Unauthorized);

                let mut op = OPERATORS.load(deps.storage, &msg.operator)?;
                // TODO: verify proof against operator's signed commitment
                op.slashed = true;
                let slash_amount = op.stake;
                op.stake = U256::ZERO;
                OPERATORS.save(deps.storage, &msg.operator, &op)?;

                Ok(Response::new()
                    .add_event(OperatorSlashed { operator: msg.operator, amount: slash_amount })
                    // Send slashed funds to treasury via bank msg
                    .add_message(BankMsg::Send {
                        to: owner,
                        amount: vec![Coin { denom: "ulayer".into(), amount: slash_amount }],
                    }))
            }

            ExecuteMsg::registerService(msg) => {
                let owner = OWNER.load(deps.storage)?;
                ensure!(info.sender == owner, EwasmError::Unauthorized);

                let service_id = U256::from_be_bytes(msg.component_hash.0);
                SERVICES.save(deps.storage, &service_id, &ServiceConfig {
                    component_hash: msg.component_hash,
                    quorum_threshold: msg.quorum_threshold,
                    operator_count: 0,
                })?;

                Ok(Response::new()
                    .add_event(ServiceRegistered { service_id, component_hash: msg.component_hash }))
            }

            _ => Err(EwasmError::UnknownMessage),
        }
    }

    pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Bytes> {
        match msg {
            QueryMsg::getOperator(msg) => {
                let op = OPERATORS.load(deps.storage, &msg.operator)?;
                Ok((op.signing_key, op.stake, op.registered_at, op.slashed).abi_encode().into())
            }
            QueryMsg::isOperator(msg) => {
                let exists = OPERATORS.has(deps.storage, &msg.operator);
                Ok(exists.abi_encode().into())
            }
            QueryMsg::getService(msg) => {
                let svc = SERVICES.load(deps.storage, &msg.service_id)?;
                Ok((svc.component_hash, svc.quorum_threshold, svc.operator_count).abi_encode().into())
            }
        }
    }
}
```

## Patterns Shown

- **Staking via `info.funds`** — same pattern as CosmWasm `payable` execute
- **Cross-contract messaging** — `BankMsg::Send` to move slashed funds
- **Composite map keys** — `Map<Address, OperatorInfo>` keyed by EVM address
- **Authorization** — `ensure!(info.sender == owner, ...)` guards on execute
- **Derived IDs** — `service_id` derived from `component_hash` (deterministic)
