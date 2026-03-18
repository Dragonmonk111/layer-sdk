# Architecture Research

**Domain:** Rust blockchain — consensus migration, Ewasm runtime, WAVS AVS state, zkVM rollup
**Researched:** 2026-03-18
**Confidence:** MEDIUM (Commonware docs are sparse on finalization callbacks; wreth not publicly indexed; WAVS bidirectional state is implementation-defined)

---

## Standard Architecture

### System Overview (Target State)

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                             Client Layer                                      │
│  ┌──────────────┐  ┌──────────────────┐  ┌──────────────────────────────┐   │
│  │  REST / JSON │  │   gRPC (Ethereum  │  │  WAVS Trigger Contracts      │   │
│  │  Gateway(Go) │  │   ABI types)      │  │  (EigenLayer AVS + Layer)    │   │
│  └──────┬───────┘  └────────┬─────────┘  └──────────────┬───────────────┘   │
└─────────┼───────────────────┼────────────────────────────┼───────────────────┘
          │                   │                            │
┌─────────▼───────────────────▼────────────────────────────▼───────────────────┐
│                          slay3rd Daemon                                        │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │                     Commonware Consensus Layer                            │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │ │
│  │  │  Automaton   │  │    Relay     │  │   Reporter   │  │  p2p::auth  │ │ │
│  │  │  (app impl)  │  │  (broadcast) │  │ (observ.)    │  │  (peers)    │ │ │
│  │  └──────┬───────┘  └──────────────┘  └──────────────┘  └─────────────┘ │ │
│  │         │ propose / verify                                               │ │
│  │         ▼                                                                │ │
│  │  ┌──────────────────────────────────────────────────────────────────┐   │ │
│  │  │            consensus::threshold_simplex Engine                    │   │ │
│  │  │   (BFT agreement; emits finalization certificates per view)       │   │ │
│  │  └──────────────────────────────────────────────────────────────────┘   │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │                    Application State Machine (packages/app)               │ │
│  │  ┌──────────┐  ┌──────────┐  ┌────────────────────────────────────────┐ │ │
│  │  │  Auth    │  │  Bank    │  │  Ewasm Module (replaces CosmWasm)      │ │ │
│  │  │  module  │  │  module  │  │  ┌──────────────┐ ┌─────────────────┐ │ │ │
│  │  │(Ethereum │  │(Ethereum │  │  │  Wasmtime or │ │  EEI host fns   │ │ │ │
│  │  │ accounts)│  │ balances)│  │  │  wasmer      │ │  (storage, call)│ │ │ │
│  │  └──────────┘  └──────────┘  │  └──────────────┘ └─────────────────┘ │ │ │
│  │                               │  Contracts: Rust/AssemblyScript→WASM  │ │ │
│  │                               │  Types: Ethereum ABI (not Cosmos)     │ │ │
│  │                               └────────────────────────────────────────┘ │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐ │
│  │                        Storage Layer (packages/storage)                   │ │
│  │  ┌──────────────────────────┐  ┌───────────────────────────────────────┐ │ │
│  │  │  RockStore (RocksDB)     │  │  MemoryStore (testing / fallback)     │ │ │
│  │  └──────────────────────────┘  └───────────────────────────────────────┘ │ │
│  └──────────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────────┘
          │ periodic state commitment                         ▲ WAVS reads state
          ▼                                                   │
┌──────────────────────────────────────────────────────────────────────────────┐
│                         WAVS AVS Layer (external)                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │  Trigger     │  │  WASI        │  │  Aggregator  │  │  Verifier       │ │
│  │  Contract    │  │  Runtime     │  │  (collects   │  │  Contract       │ │
│  │  (on-chain)  │  │  (operators) │  │  signatures) │  │  (validates)    │ │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────────┘ │
│                                          │ signed results submitted on-chain  │
└──────────────────────────────────────────┼───────────────────────────────────┘
                                           │
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                         zkVM Rollup Layer (wreth)                             │
│  ┌─────────────────────────┐   ┌──────────────────────────────────────────┐ │
│  │  Guest Program (Rust)   │   │  Host / Prover                           │ │
│  │  - reads Layer state    │   │  - runs guest in SP1 or RISC Zero        │ │
│  │  - executes STF         │   │  - generates STARK proof                 │ │
│  │  - commits state root   │   │  - compresses to Groth16 / Plonk         │ │
│  └─────────────────────────┘   └──────────────────────────────────────────┘ │
│                                           │ proof + state root               │
└───────────────────────────────────────────┼──────────────────────────────────┘
                                            ▼
                              ┌─────────────────────────┐
                              │  Ethereum L1            │
                              │  On-chain Verifier      │
                              │  (Groth16/Plonk Solidity│
                              │   contract)             │
                              └─────────────────────────┘
```

---

## Question 1: Commonware — What Replaces ABCI?

### Summary

ABCI is a monolithic request/response interface between a consensus engine (CometBFT) and an application. Commonware does not have an equivalent — it provides **composable primitives** that the application assembles, not a framework the application plugs into.

The application boundary moves: rather than implementing a fixed interface that a consensus engine calls, the application *is* the driver. It implements traits that consensus::simplex or consensus::threshold_simplex calls at specific moments.

### New Application Boundary: The Automaton Trait

The primary integration point is the `Automaton` trait (HIGH confidence — from docs.rs):

```rust
// Application implements this to drive consensus
trait Automaton {
    type Context;   // metadata: proposer, view, height, epoch
    type Digest;    // hash of a block payload

    async fn genesis(&mut self, epoch: Epoch) -> Self::Digest;
    async fn propose(&mut self, context: Self::Context) -> Receiver<Self::Digest>;
    async fn verify(&mut self, context: Self::Context, payload: Self::Digest) -> Receiver<bool>;
}
```

**What this means for Layer:**
- `propose()` is called on the leader each view — the application constructs the block payload (ordered transactions) and returns its digest
- `verify()` is called for all received proposals — the application validates the block before voting
- `genesis()` bootstraps the initial state

The consensus engine operates on **opaque binary blobs** (`Vec<u8>`). Block format, transaction encoding, and state transitions are entirely defined by the application. There is no prescribed block structure.

### Supporting Traits

| Trait | Purpose | Current Equivalent |
|-------|---------|-------------------|
| `Relay` | Broadcast full payloads to peers (consensus works on digests) | ABCI `BroadcastTx` + P2P gossip |
| `Reporter` | Receive consensus activity notifications (votes, faults) | No direct equivalent in ABCI |
| `CertifiableAutomaton` | Delay/gate finalization between notarization and commit | ABCI `ProcessProposal` |
| `VerifyingApplication` | Verify blocks relative to their ancestry | ABCI `ProcessProposal` |

### Finalization Signal (MEDIUM confidence)

Based on the simplex documentation: finalization occurs after 2f+1 `finalize(c,v)` votes are collected. The consensus engine notifies the application when a block is finalized. The exact callback mechanism is not fully documented in public API docs but is indicated to be async notification via the application's trait implementation.

The pattern matches Alto's architecture: the `chain` module receives finalized blocks and applies them to state.

### What the Application Must Build (Not Provided by Commonware)

- **Mempool**: Transaction ordering and selection — no CometBFT mempool exists
- **Block construction**: Format of `propose()` payload is application-defined
- **State execution**: Applying transactions on finalization — no ABCI `FinalizeBlock` equivalent
- **Peer-aware validators**: Commonware's `p2p::authenticated` manages peers by public key, but the application must maintain the validator set
- **gRPC / query interface**: No automatic gRPC endpoint generation

### Mapping Current Code to New Boundary

| Current (ABCI) | New (Commonware) |
|----------------|-----------------|
| `packages/abci/src/application.rs` — `Application` trait | Delete; replaced by `Automaton` impl |
| `app/slay3rd/src/app.rs` — `Pulsarium` ABCI impl | Replace with new struct implementing `Automaton` |
| `check_tx()` | App-managed mempool validation |
| `finalize_block()` | Finalization callback from consensus engine |
| `commit()` | Collapse into finalization handler |
| `prepare_proposal()` | Becomes `propose()` |
| `process_proposal()` | Becomes `verify()` |
| ABCI socket server | Removed; runtime is Commonware's `runtime::tokio` |

---

## Question 2: Ewasm Runtime — Execution Model

### Summary

Ewasm is a restricted, deterministic subset of WebAssembly for blockchain smart contracts. The execution model is **in-process** — the WASM runtime runs embedded inside the blockchain node, not as a separate process. Communication between contracts and the host blockchain happens through **host functions** (the Ethereum Environment Interface, EEI).

### Execution Model (HIGH confidence — ewasm official docs)

```
Node Process
├── State Machine (packages/app)
│   └── Ewasm Module
│       ├── WASM Runtime (wasmtime / wasmer) — in-process
│       │   └── Contract WASM module
│       │       ├── Imports: EEI host functions
│       │       │   ├── storageLoad(key_ptr) → val_ptr
│       │       │   ├── storageStore(key_ptr, val_ptr)
│       │       │   ├── call(gas, addr_ptr, val_ptr, data_ptr, data_len)
│       │       │   ├── getBlockNumber() → i64
│       │       │   ├── getCallValue(result_ptr)
│       │       │   └── ... (~40 EEI functions)
│       │       └── Exports: main entry points
│       │           ├── call()    — invoked for calls/transactions
│       │           └── deploy()  — invoked for instantiation
│       └── Host implements EEI functions as Rust closures
```

### Key Differences from CosmWasm

| Aspect | CosmWasm (current) | Ewasm (target) |
|--------|-------------------|----------------|
| Address format | 32-byte bech32 | 20-byte Ethereum hex |
| ABI encoding | JSON (serde) | Ethereum ABI (ethabi / alloy) |
| Storage key | `cosmwasm_std::Storage` trait | 256-bit key/value (EEI) |
| Host interface | CosmWasm host functions | EEI (~40 standard functions) |
| Entrypoint | `instantiate`, `execute`, `query` | `call()`, `deploy()` |
| Floats | Allowed | Disallowed (non-deterministic) |
| Cross-contract calls | `WasmMsg::Execute` | `call()`, `callDelegate()` |

### What "Ethereum Types" Means for Layer

The migration from CosmWasm to Ewasm means:
- Accounts identified by 20-byte Ethereum addresses (not Cosmos bech32)
- Transaction signing uses secp256k1 with Ethereum's signature scheme (v, r, s)
- Contract state uses 256-bit slot-based storage (EEI `storageLoad`/`storageStore`)
- Contract interactions use Ethereum ABI encoding for inputs/outputs
- The `packages/cosmos` and `packages/proto` packages (Cosmos-specific) are largely superseded

### Implementation Path for the Wasm Module

The existing `packages/app/src/wasm/` structure is the right boundary. The `keeper.rs` and `vm/` subdirectory are the primary replacement targets:

```
packages/app/src/wasm/
├── keeper.rs          — replace CosmWasm VM calls with Ewasm VM calls
├── vm/
│   ├── mod.rs         — replace wasmvm with wasmtime/wasmer
│   ├── backend.rs     — replace CosmWasm backend with EEI implementation
│   └── cache.rs       — retain; module caching is runtime-agnostic
└── events.rs          — update event types (Ethereum logs vs CosmWasm events)
```

**The `packages/storage/` layer does not change** — the EEI host functions translate 256-bit slot reads/writes into prefixed key-value operations on the existing `PersistentStorage` trait. The storage abstraction is clean enough to survive this migration.

---

## Question 3: WAVS Bidirectional State Architecture

### Summary

WAVS is a WASI-based off-chain compute runtime for AVS services on EigenLayer. "Bidirectional" in Layer's context means:

- **Layer → WAVS (read)**: WAVS services trigger on Layer state (block events, contract state). Layer is a trigger source.
- **WAVS → Layer (write)**: AVS operators run services off-chain, sign results, and submit them to Layer via on-chain contracts. Layer is a submission target.

### WAVS Component Architecture (MEDIUM confidence — docs.wavs.xyz + github)

```
WAVS Service Lifecycle
─────────────────────

1. TRIGGER (on-chain event on Layer or Ethereum)
   │
   ▼
2. SERVICE COMPONENT (Rust → WASM/WASI)
   - Executes in sandboxed WASI runtime on operator machines
   - Can read external state, call APIs, run compute
   - Receives trigger data as input
   │
   ▼
3. OPERATOR SIGNATURE
   - Each operator signs the computation result
   - Independent execution, deterministic output
   │
   ▼
4. AGGREGATION
   - Aggregator contract collects N-of-M operator signatures
   - Verifier contract validates quorum (configurable threshold)
   │
   ▼
5. ON-CHAIN SUBMISSION (to Layer)
   - Verified result committed to Layer state via task/verifier contracts
   - Layer state is now updated with off-chain computation result
```

### State Commitment Location

Based on the `commitments` repo found at `/Users/jacobhartnell/Dev/projects/Layer/commitments/`:

Layer's infrastructure contracts define a **smart lien / restaking architecture** where:
- **AVS contracts** live on Layer (tasks, verifier-simple, operator registry)
- **Operator results** are submitted through task queue contracts
- **Verification** happens on-chain via quorum rules in `verifier-simple`

The task queue pattern (`contracts/avs/tasks/`) is the primary state commitment point: an AVS operator submits a result which the verifier contract checks against operator voting power and quorum.

### What "Bidirectional" Requires from the Layer Blockchain

| Direction | Mechanism | Where Implemented |
|-----------|-----------|-------------------|
| Layer state → WAVS operator | Block event triggers in WAVS trigger system | WAVS trigger contracts (separate AVS infra) |
| Layer state → WAVS operator | RPC/query of Layer contract state | Existing gRPC query layer (survives migration) |
| WAVS result → Layer state | Operator submits tx to Layer | Layer tx processing (Ewasm contracts handle it) |
| Operator signature aggregation | On-chain verifier contract | `contracts/avs/verifier-simple` (in commitments repo) |

**Key implication**: The Layer state machine needs to support the AVS infrastructure contracts (tasks, verifier, operator registry). These are Ewasm contracts deployed to Layer — not special host-level logic. The Ewasm migration enables this directly.

### WAVS State Commitment Architecture

```
Layer Blockchain
├── AVS Task Contract (Ewasm)
│   ├── accepts task requests (trigger point for operators)
│   └── stores pending tasks
├── Verifier Contract (Ewasm)
│   ├── receives operator result submissions
│   ├── checks operator voting power (from operator registry)
│   ├── enforces quorum threshold
│   └── on success: writes result to task contract
└── Operator Registry Contract (Ewasm)
    └── tracks registered operators and their weights
```

Operators query this state, run WASI services off-chain, then submit signed results back as Layer transactions.

---

## Question 4: wreth / zkVM State Rollup Architecture

### Summary

"wreth" is not publicly indexed — it is likely an internal Layer project name, possibly a Rust Ethereum node (reth-derivative) used as the proving target. The general architecture is well-established from SP1 and RISC Zero, both of which support Rust guest programs and Ethereum on-chain verification.

### zkVM Proof Flow (HIGH confidence — SP1 + RISC Zero official docs)

```
Data Flow: Layer State → Proof → Ethereum

1. WITNESS COLLECTION (host)
   Layer node exports:
   - Block headers (hash, state root, tx root)
   - Execution witness: all pre-state MPT nodes touched
   - Transaction list for the proven batch
   │
   ▼
2. GUEST PROGRAM (Rust, compiled to RISC-V ELF)
   Runs inside zkVM (SP1 or RISC Zero):
   - Re-executes all transactions stateless (from witness)
   - Applies state transitions (Layer's STF)
   - Computes new state root
   - Commits (state_root_before, state_root_after, block_range) to journal
   │
   ▼
3. PROOF GENERATION (host prover)
   - Executor records RISC-V execution trace → session
   - Prover generates STARK proof of correct execution
   - Compressor converts STARK → Groth16 (BN254) for EVM compatibility
   - Output: (proof, journal/public_inputs)
   │
   ▼
4. ON-CHAIN SUBMISSION (Ethereum)
   Proof submission tx contains:
   - Groth16 proof bytes
   - Public inputs: (state_root_before, state_root_after, block_range)
   - Image ID (cryptographic ID of the guest ELF binary)
   │
   ▼
5. ETHEREUM VERIFIER CONTRACT (Solidity)
   - Verifies Groth16 proof against known Image ID
   - Checks state_root_before matches last committed root
   - Updates committed state root on success
   - Emits StateRootUpdated event
```

### zkVM Choice (MEDIUM confidence — research pending)

The PROJECT.md states "zkVM TBD during research." Based on available evidence:

| zkVM | Rust Guest | Ethereum Verifier | Proving Speed | Status |
|------|------------|-------------------|---------------|--------|
| SP1 (Succinct) | Yes (std Rust) | Yes (Groth16) | ~10s Ethereum blocks | Production |
| RISC Zero | Yes (no_std Rust) | Yes (Groth16) | ~44s Ethereum blocks | Production |

**Recommendation**: SP1 because it supports `std` Rust (simpler guest program development) and has real-time proving for Ethereum-scale workloads. RISC Zero is the alternative if wreth has existing RISC Zero integration.

The zeth project (RISC Zero's Ethereum block prover using reth) is the closest architectural reference for what wreth likely does, using reth's stateless execution inside the guest.

### What Layer State Must Provide for zkVM Proving

The zkVM guest needs:
1. **State root** — the Merkle root of Layer's state trie after each block
2. **Execution witness** — all MPT nodes accessed during block execution (for stateless re-execution in guest)
3. **Transaction data** — ordered list of txs in the proven block range

This requires Layer's storage layer to support **Merkle Patricia Trie** (or equivalent) state commitment. The current RocksDB storage layer does not produce state roots — it stores raw key-value pairs. Adding MPT or a sparse Merkle tree for state commitments is a prerequisite for zkVM proving.

---

## Question 5: What to Preserve vs. Replace

### Preserve (HIGH confidence)

| Component | Location | Why Preserve |
|-----------|----------|-------------|
| Storage trait + RocksDB | `packages/storage/` | Clean abstraction; EEI host fns map directly to it; MemoryStore survives for testing |
| State machine module system | `packages/app/src/{auth,bank,wasm}/` | Keeper pattern maps cleanly to new consensus; module boundaries remain valid |
| `App<T: PersistentStorage>` struct | `packages/app/src/app.rs` | Core state container; replace method bodies, keep structure |
| Error handling patterns | `packages/app/src/error.rs` | Result-based error propagation is sound |
| Gas metering | `packages/app/src/` | Gas metering concept survives; values and costs change |
| gRPC service layer | `app/slay3rd/src/grpc/` | Keep layer structure; update from Cosmos types to Ethereum types |
| REST gateway (Go) | `gateway/` | Update proto definitions; gateway translation survives |
| Testing infrastructure | `packages/app/src/testing/` | MemoryStore-based tests remain valid |
| Docker setup | `localnode/`, `docker/` | Replace CometBFT container; node startup structure survives |

### Must Replace

| Component | Location | Why Replace |
|-----------|----------|------------|
| ABCI package | `packages/abci/` | Entirely CometBFT-specific; Commonware has no equivalent |
| Pulsarium ABCI app | `app/slay3rd/src/app.rs` | Replace with Commonware `Automaton` implementation |
| ABCI encode/decode | `app/slay3rd/src/{encode,decode}.rs` | ABCI proto types go away |
| CosmWasm VM | `packages/app/src/wasm/vm/` | Replace with Ewasm runtime (wasmtime/wasmer + EEI) |
| Cosmos types | `packages/cosmos/`, `packages/proto/` | Replace with Ethereum types (alloy / ethabi) |
| Cosmos-specific proto | `proto/cosmos/`, `proto/cosmwasm/` | Replace with Ethereum ABI or custom proto for gRPC |
| `packages/std/` Cosmos types | `packages/std/src/api/` | Update Msg/Query types from Cosmos to Ethereum format |
| Tendermint gRPC services | `app/slay3rd/src/grpc/tendermint.rs` | Remove; replace with Commonware node info API |
| CometBFT localnode config | `localnode/comet/` | Remove; replace with Commonware node config |
| CosmWasm contracts | `contracts/` | Replace with Ewasm contracts (Rust→WASM with EEI) |

### Partially Reuse

| Component | Keep | Replace |
|-----------|------|---------|
| `packages/std/` | `Block`, `TxResult` structures | Address types (20-byte), encoding (ABI not JSON) |
| Auth module | Nonce tracking, account management | Signature scheme (Ethereum secp256k1), address format |
| Bank module | Balance tracking pattern, prefixed storage | Denom handling, Ethereum wei denomination |
| gRPC handlers | Service registration, tonic setup | Request/response types (Cosmos proto → Ethereum ABI) |

---

## Architectural Patterns

### Pattern 1: Automaton as Consensus Bridge

**What:** A new struct (replaces `Pulsarium`) implements `Automaton` for Commonware. It holds `Arc<RwLock<App<T>>>` identically to the current design, but instead of responding to ABCI requests, it responds to `propose()` and `verify()` calls from the consensus engine.

**When to use:** The single integration point between Commonware consensus and the application state machine.

**Trade-offs:** Application must manage its own mempool. `propose()` must be non-blocking (returns a channel). Finalization is push-based, not pull-based.

**Example structure:**
```rust
pub struct LayerNode<T: PersistentStorage + 'static> {
    app: Arc<RwLock<App<T>>>,
    mempool: Arc<RwLock<Mempool>>,
}

impl<T: PersistentStorage + 'static> Automaton for LayerNode<T> {
    type Context = commonware_consensus::Context;
    type Digest = [u8; 32];

    async fn propose(&mut self, ctx: Self::Context) -> Receiver<Self::Digest> {
        // Pull txs from mempool, construct block, return digest
    }

    async fn verify(&mut self, ctx: Self::Context, digest: Self::Digest) -> Receiver<bool> {
        // Validate proposed block against current state
    }
}
```

### Pattern 2: EEI Host Functions as Storage Bridge

**What:** The Ewasm module implements EEI host functions as closures/callbacks that translate 256-bit slot storage into prefixed RocksDB keys. No separate process or IPC needed — the WASM runtime is embedded in the state machine.

**When to use:** Every contract storage read/write.

**Trade-offs:** Synchronous host function calls block the WASM execution thread. This is correct behavior (matches EVM semantics). Floating-point must be disabled in the WASM runtime configuration.

**Example:**
```rust
// EEI host function registered with wasmtime linker
fn storage_store(
    mut caller: Caller<'_, EeiContext>,
    key_ptr: i32,
    val_ptr: i32,
) {
    let ctx = caller.data_mut();
    let key = ctx.read_memory(key_ptr, 32);    // 256-bit key
    let val = ctx.read_memory(val_ptr, 32);    // 256-bit value
    ctx.storage.set(&prefixed_key(&ctx.contract_addr, &key), &val);
}
```

### Pattern 3: WAVS Operator Result Submission

**What:** AVS operators run WASI services off-chain, sign results deterministically, and submit them to Layer as standard transactions targeting the AVS verifier contract. The verifier contract enforces quorum and commits results.

**When to use:** Any AVS computation that needs on-chain settlement.

**Trade-offs:** Results only reach Layer state if quorum is met. Latency includes off-chain compute time + operator signing + aggregation. Operators must be registered on-chain with staked collateral.

### Pattern 4: zkVM Stateless Block Re-execution

**What:** A Rust guest program takes an execution witness (pre-state MPT nodes) + block transactions, re-executes the block inside the zkVM, and commits the resulting state root to the proof's public journal.

**When to use:** Each proof submission to Ethereum.

**Trade-offs:** Requires Layer to generate execution witnesses (pre-state nodes accessed during block execution). Requires state root via Merkle tree — current RocksDB storage does not produce state roots. This is a significant infrastructure addition.

---

## Data Flow

### Flow 1: Transaction Processing (Target State)

```
Client → gRPC/REST
    ↓
LayerNode mempool (application-managed, no CometBFT mempool)
    ↓
propose() called by Commonware on block leader
    ↓
LayerNode builds block payload (ordered txs + state root)
    ↓
verify() called on all validators (validates payload against local state)
    ↓
consensus::threshold_simplex: 2f+1 notarize votes → notarization
    ↓
CertifiableAutomaton::certify() — approve finalization
    ↓
2f+1 finalize votes → finalization certificate
    ↓
Application receives finalization notification
    ↓
App::finalize_block() — executes txs, updates RocksDB state
    ↓
New state root committed; Relay broadcasts to peers
```

### Flow 2: WAVS AVS State Write

```
Layer block event (contract emits event) / cron schedule
    ↓
WAVS trigger fires → operator WASI runtime receives trigger data
    ↓
Service component executes off-chain computation
    ↓
Each operator signs result (deterministic, reproducible)
    ↓
Aggregator collects N-of-M signatures → verifier contract checks quorum
    ↓
Verified result submitted as Layer transaction
    ↓
Ewasm verifier contract executes on Layer
    ↓
Result stored in AVS task contract state (on Layer)
```

### Flow 3: zkVM State Rollup to Ethereum

```
Layer block finalized (Commonware consensus)
    ↓
Witness collector captures: pre-state MPT nodes + txs
    ↓
wreth (host) feeds witness + block to zkVM prover
    ↓
Guest program (Rust): re-executes block → computes new state root → commits to journal
    ↓
Prover generates STARK proof
    ↓
Compressor converts STARK → Groth16 (BN254)
    ↓
Submission tx sent to Ethereum: (proof, state_root_before, state_root_after, block_range)
    ↓
Ethereum Solidity verifier contract: verifies proof → accepts new state root
```

---

## Recommended Project Structure (Target)

```
layer-sdk/
├── app/
│   └── slay3rd/src/
│       ├── main.rs          # startup: commonware runtime, storage, node
│       ├── node.rs          # LayerNode: implements Automaton (replaces app.rs)
│       ├── mempool.rs       # Application-managed tx mempool (new)
│       ├── config.rs        # keep; update for Commonware config
│       ├── relay.rs         # implements Relay trait for commonware (new)
│       └── grpc/            # keep structure; update types
│           ├── mod.rs
│           ├── tx.rs        # Ethereum tx submission
│           ├── eth.rs       # Ethereum account/balance queries
│           └── node.rs      # Node info (replaces tendermint.rs)
├── packages/
│   ├── app/src/             # PRESERVE structure; update internals
│   │   ├── app.rs           # keep App<T> wrapper; update execute()
│   │   ├── sm.rs            # update routing for Ethereum msg types
│   │   ├── auth/            # update to Ethereum accounts (20-byte addr)
│   │   ├── bank/            # update to wei denomination
│   │   └── wasm/            # REPLACE internals; keep module boundary
│   │       ├── keeper.rs    # replace CosmWasm calls with Ewasm calls
│   │       └── vm/          # replace: wasmtime + EEI host functions
│   ├── storage/             # PRESERVE as-is
│   ├── ewasm/               # NEW: EEI host function implementations
│   │   └── src/
│   │       ├── eei.rs       # EEI host function definitions
│   │       ├── context.rs   # per-call execution context
│   │       └── meter.rs     # gas metering for EEI calls
│   ├── std/                 # PARTIALLY update: Ethereum types
│   │   └── src/api/
│   │       ├── msg.rs       # Ethereum ABI-encoded messages
│   │       └── account.rs   # Ethereum account type (20-byte addr)
│   └── proto/               # UPDATE: remove Cosmos/Tendermint protos
├── contracts/               # REPLACE: Ewasm contracts (Rust→WASM)
│   ├── avs-tasks/           # Layer AVS task queue (Ewasm)
│   ├── avs-verifier/        # AVS result verifier (Ewasm)
│   └── operator-registry/   # Operator registration (Ewasm)
└── packages/abci/           # DELETE after migration
```

---

## Component Boundaries

| Boundary | Communication | Direction | Notes |
|----------|--------------|-----------|-------|
| Commonware ↔ LayerNode | Automaton trait calls | Consensus → App | propose(), verify(), finalization notification |
| LayerNode ↔ App<T> | Direct method calls (Arc<RwLock>) | Sync | Same pattern as current Pulsarium |
| App ↔ Ewasm Module | Direct fn call | Sync | Module keeper called per transaction |
| Ewasm Runtime ↔ EEI | Host function callbacks | Sync (in-process) | Wasmtime linker registers Rust fns as imports |
| EEI ↔ Storage | Trait method calls | Sync | EEI translates 256-bit slots to prefixed keys |
| Layer ↔ WAVS | On-chain transactions | Async (external) | Operators submit txs; Layer emits events |
| Layer ↔ wreth | Block data / witness export | Async (IPC or RPC) | Implementation TBD; likely gRPC or shared DB |
| wreth ↔ Ethereum | Proof submission tx | Async (external) | Groth16 proof + public inputs |

---

## Build Order Implications

The component dependency graph drives phase ordering:

```
Phase 1: Storage + Type Foundation
  RocksDB works → Ethereum address types → Auth/Bank with Ethereum accounts
  (no consensus needed; test with MemoryStore)

Phase 2: Commonware Consensus Integration
  Depends on: Phase 1 (state machine must exist for propose/verify)
  Replaces: packages/abci, Pulsarium
  New: LayerNode (Automaton), Relay, mempool management
  Risk: Commonware is ALPHA; expect API changes

Phase 3: Ewasm Runtime (CosmWasm → Ewasm)
  Depends on: Phase 1 (storage), Phase 2 optional (can test in isolation)
  Replaces: packages/app/src/wasm/vm/
  New: packages/ewasm (EEI host functions), wasmtime linker setup
  Risk: CosmWasm submodule fork must bridge both runtimes during migration

Phase 4: WAVS State Integration
  Depends on: Phase 3 (Ewasm contracts must work to deploy AVS contracts)
  New: AVS task/verifier/operator contracts (Ewasm), WAVS trigger configuration
  Risk: Bidirectional state requires Layer to emit events WAVS can trigger on

Phase 5: zkVM State Rollup
  Depends on: Phase 1 (need Merkle state root — significant addition to storage)
  New: MPT / sparse Merkle tree in storage layer, wreth witness export, guest program
  Risk: Highest unknown — wreth architecture is not publicly documented; zkVM choice TBD
```

---

## Anti-Patterns

### Anti-Pattern 1: Porting ABCI Concepts Literally to Commonware

**What people do:** Try to map `check_tx → mempool`, `finalize_block → finalization callback`, `commit → post-finalization` as a 1:1 translation.

**Why it's wrong:** Commonware's application boundary is lower. The application manages its own mempool and defines its own block format. Forcing ABCI concepts into Commonware traits creates impedance mismatch and unnecessary complexity.

**Do this instead:** Design the `LayerNode` (Automaton impl) as a clean state machine driver. The mempool is an application concern, block structure is application-defined, and finalization is an async notification. Don't try to recover the ABCI boundary.

### Anti-Pattern 2: Separate Process for Ewasm

**What people do:** Run the WASM runtime as a sidecar process, communicating via IPC (like the original CosmWasm/wasmvm FFI setup).

**Why it's wrong:** Ewasm's EEI is synchronous by design. IPC introduces latency and complexity. The determinism guarantees are harder to enforce across process boundaries.

**Do this instead:** Embed wasmtime or wasmer as an in-process library. Register EEI host functions as Rust closures via the linker. This is how Substrate does it, and it matches the EEI design intent.

### Anti-Pattern 3: Building zkVM Proving Without State Roots

**What people do:** Start the zkVM integration assuming the guest can read state from RocksDB directly.

**Why it's wrong:** zkVM guest programs must be stateless — they prove execution from a witness. Without Merkle state roots, you cannot generate a witness, and without a witness, the guest cannot re-execute the block in a verifiable way.

**Do this instead:** Add a Merkle state commitment layer (sparse Merkle tree or MPT) to the storage layer before starting zkVM work. This is a prerequisite, not a parallel workstream.

### Anti-Pattern 4: WAVS Bidirectional State via Database Polling

**What people do:** Have WAVS operators poll Layer's RocksDB directly via some exported file path.

**Why it's wrong:** Breaks the blockchain's consistency guarantees. State reads must go through the canonical query path to be atomic with respect to block finalization.

**Do this instead:** WAVS operators read Layer state via the gRPC query interface (or Ethereum-compatible RPC after the type migration). Layer emits trigger events through standard contract emit mechanisms that the WAVS trigger system monitors.

---

## Integration Points

### External Services

| Service | Integration Pattern | Notes |
|---------|---------------------|-------|
| Commonware p2p | Peer registry via BLS/ECDSA public key list | Validator set managed by application (no automatic discovery) |
| WAVS operator network | On-chain tx submission from operators | Operators are external; Layer is passive recipient |
| Ethereum L1 | Proof submission tx via wreth | One-directional; Layer proves to Ethereum, not vice versa |
| EigenLayer restaking | Via WAVS middleware contracts | Mirrors EigenLayer state on Layer (cross-chain messaging TBD) |

### Internal Boundaries That Must Remain Clean

| Boundary | Communication | Constraint |
|----------|--------------|------------|
| Consensus ↔ App | `Automaton` trait only | No direct storage access from consensus engine |
| App ↔ Storage | `PersistentStorage` trait only | Modules cannot bypass the trait |
| Ewasm ↔ Host | EEI functions only | No direct memory sharing; all via EEI callbacks |
| Modules ↔ Each Other | Through `App` dispatcher | No module-to-module direct calls; all go through execute() routing |

---

## Confidence Assessment

| Area | Confidence | Source | Notes |
|------|------------|--------|-------|
| Commonware Automaton interface | HIGH | docs.rs/commonware-consensus | Trait signatures confirmed |
| Commonware finalization callback | MEDIUM | Docs + Alto reference | Exact callback mechanism not fully documented |
| Ewasm EEI host function model | HIGH | ewasm.readthedocs.io official | Design spec confirmed |
| Ewasm in-process execution | HIGH | ewasm design docs + Substrate precedent | Standard pattern |
| WAVS trigger/submit flow | MEDIUM | docs.wavs.xyz + github | High-level confirmed; bidirectional specifics need validation |
| WAVS + Layer state integration | MEDIUM | commitments repo + PROJECT.md | Architecture inferred from contracts repo |
| SP1/RISC Zero proof flow | HIGH | Official docs + github | Well-documented; wreth specific TBD |
| wreth architecture | LOW | Not publicly indexed | Internal project; architecture is inferred from zkVM patterns |
| Merkle state root requirement | HIGH | Standard zkVM rollup pattern | Prerequisite is clear regardless of zkVM choice |
| Storage layer preservation | HIGH | Code analysis | PersistentStorage trait is clean; migration-safe |

---

## Sources

- [commonware-consensus docs.rs](https://docs.rs/commonware-consensus/latest/commonware_consensus/) — Automaton, Relay, Reporter trait definitions
- [Commonware Anti-Framework blog](https://commonware.xyz/blogs/commonware-the-anti-framework) — philosophy and design intent
- [Commonware threshold_simplex](https://docs.rs/commonware-consensus/latest/commonware_consensus/threshold_simplex/index.html) — BFT agreement with threshold signatures
- [Alto blockchain (reference implementation)](https://github.com/commonwarexyz/alto) — minimal blockchain built with Commonware
- [Ewasm Design Specification](https://github.com/ewasm/design) — official EEI specification
- [Ethereum Environment Interface functions](https://ewasm.readthedocs.io/en/mkdocs/eth_interface/) — host function reference
- [WAVS Overview docs](https://docs.wavs.xyz/overview) — trigger/compute/submit architecture
- [WAVS on Layer announcement](https://www.layer.xyz/news-and-insights/introducing-wavs-the-next-gen-avs-builder) — bidirectional state intent
- [WAVS middleware (Lay3rLabs)](https://github.com/Lay3rLabs/wavs-middleware) — aggregator/verifier contracts
- [SP1 zkVM introduction](https://blog.succinct.xyz/introducing-sp1/) — Rust guest programs for proof generation
- [RISC Zero zkVM docs](https://dev.risczero.com/api/zkvm/) — host/guest model
- [RISC Zero Ethereum contracts](https://github.com/risc0/risc0-ethereum) — on-chain Groth16 verifier
- [zeth Ethereum block prover](https://github.com/risc0/zeth) — architectural reference for Layer block proving
- [SP1 vs RISC Zero comparison](https://medium.com/@gwrx2005/comparative-analysis-of-sp1-and-risc-zero-zero-knowledge-virtual-machines-4abf806daa70) — zkVM selection criteria
- Layer commitments repo (`/Users/jacobhartnell/Dev/projects/Layer/commitments/`) — AVS task/verifier/operator contract architecture
- Layer SDK existing codebase (`/Users/jacobhartnell/Dev/projects/Layer/layer-sdk/`) — current ABCI + storage architecture

---

*Architecture research for: Layer SDK — consensus migration, Ewasm runtime, WAVS state, zkVM rollup*
*Researched: 2026-03-18*
