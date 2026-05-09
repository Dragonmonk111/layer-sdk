# Example: Task Mailbox

The bridge between WAVS and Layer state. Receives signed results from WAVS operators and commits them on-chain after quorum.

```rust
use layer_ewasm::*;

// --- State ---

const SERVICE_REGISTRY: Item<Address> = Item::new("registry");   // operator registry contract
const QUORUM: Item<u64> = Item::new("quorum");

// Map<task_id, Task>
const TASKS: Map<U256, Task> = Map::new("tasks");
// Map<(task_id, operator), signature>
const SUBMISSIONS: Map<(U256, Address), FixedBytes<65>> = Map::new("subs");
// Map<task_id, submission_count>
const SUB_COUNT: Map<U256, u64> = Map::new("sub_count");
// Map<task_id, finalized_result>
const RESULTS: Map<U256, Bytes> = Map::new("results");

#[derive(EwasmSerialize)]
pub struct Task {
    pub creator: Address,
    pub input: Bytes,
    pub created_at: u64,
    pub finalized: bool,
}

// --- ABI ---

sol! {
    struct InitMsg {
        address service_registry;
        uint64 quorum;
    }

    function createTask(bytes input) returns (uint256 task_id);
    function submitResult(uint256 task_id, bytes result, bytes signature);

    function getTask(uint256 task_id) returns (
        address creator, bytes input, uint64 created_at, bool finalized
    );
    function getResult(uint256 task_id) returns (bytes);

    event TaskCreated(uint256 indexed task_id, address indexed creator, bytes input);
    event ResultSubmitted(uint256 indexed task_id, address indexed operator);
    event TaskFinalized(uint256 indexed task_id, bytes result);
}

// --- Contract ---

#[ewasm_contract]
impl TaskMailbox {
    pub fn instantiate(deps: DepsMut, _env: Env, _info: MessageInfo, msg: InitMsg) -> Result<Response> {
        SERVICE_REGISTRY.save(deps.storage, &msg.service_registry)?;
        QUORUM.save(deps.storage, &msg.quorum)?;
        Ok(Response::new())
    }

    pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response> {
        match msg {
            ExecuteMsg::createTask(msg) => {
                let task_id = U256::from(env.block_height) << 128
                    | U256::from(env.block_timestamp);

                TASKS.save(deps.storage, &task_id, &Task {
                    creator: info.sender,
                    input: msg.input.clone(),
                    created_at: env.block_timestamp,
                    finalized: false,
                })?;
                SUB_COUNT.save(deps.storage, &task_id, &0u64)?;

                Ok(Response::new()
                    .add_event(TaskCreated {
                        task_id,
                        creator: info.sender,
                        input: msg.input,
                    })
                    .set_data(task_id.abi_encode()))  // return task_id to caller
            }

            ExecuteMsg::submitResult(msg) => {
                let task = TASKS.load(deps.storage, &msg.task_id)?;
                ensure!(!task.finalized, EwasmError::AlreadyFinalized);

                // Verify operator is registered — cross-contract query
                let registry = SERVICE_REGISTRY.load(deps.storage)?;
                let is_op: bool = deps.api.query_contract(
                    registry,
                    isOperatorCall { operator: info.sender }.abi_encode(),
                )?.abi_decode()?;
                ensure!(is_op, EwasmError::NotOperator);

                // Verify signature over (task_id, result)
                let msg_hash = deps.api.keccak256(
                    &(msg.task_id, &msg.result).abi_encode()
                );
                let signer = deps.api.ecrecover(&msg_hash, &msg.signature.try_into()?)?;
                ensure!(signer == info.sender, EwasmError::InvalidSignature);

                // Record submission
                ensure!(
                    !SUBMISSIONS.has(deps.storage, &(msg.task_id, info.sender)),
                    EwasmError::AlreadySubmitted
                );
                SUBMISSIONS.save(
                    deps.storage,
                    &(msg.task_id, info.sender),
                    &msg.signature.into(),
                )?;

                let count = SUB_COUNT.load(deps.storage, &msg.task_id)? + 1;
                SUB_COUNT.save(deps.storage, &msg.task_id, &count)?;

                let mut resp = Response::new()
                    .add_event(ResultSubmitted {
                        task_id: msg.task_id,
                        operator: info.sender,
                    });

                // Check quorum
                let quorum = QUORUM.load(deps.storage)?;
                if count >= quorum {
                    RESULTS.save(deps.storage, &msg.task_id, &msg.result)?;
                    let mut task = task;
                    task.finalized = true;
                    TASKS.save(deps.storage, &msg.task_id, &task)?;

                    resp = resp.add_event(TaskFinalized {
                        task_id: msg.task_id,
                        result: msg.result,
                    });
                }

                Ok(resp)
            }

            _ => Err(EwasmError::UnknownMessage),
        }
    }

    pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Bytes> {
        match msg {
            QueryMsg::getTask(msg) => {
                let t = TASKS.load(deps.storage, &msg.task_id)?;
                Ok((t.creator, t.input, t.created_at, t.finalized).abi_encode().into())
            }
            QueryMsg::getResult(msg) => {
                let result = RESULTS.load(deps.storage, &msg.task_id)?;
                Ok(result.abi_encode().into())
            }
        }
    }
}
```

## Patterns Shown

- **Cross-contract query** — `deps.api.query_contract(registry, abi_bytes)` calls the operator registry's `isOperator` query using ABI-encoded calldata
- **Signature verification** — `deps.api.keccak256` + `deps.api.ecrecover` as host functions, not contract-side crypto
- **Composite map keys** — `Map<(U256, Address), _>` for per-operator-per-task tracking
- **Quorum logic** — count submissions, finalize when threshold reached
- **Return data** — `Response::set_data(task_id.abi_encode())` returns ABI-encoded data to the caller (like Solidity return values)
- **Task ID derivation** — deterministic from block height + timestamp (could also use a counter)
