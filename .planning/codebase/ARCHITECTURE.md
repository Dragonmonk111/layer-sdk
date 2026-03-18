# Architecture

**Analysis Date:** 2026-03-18

## Pattern Overview

**Overall:** Layered architecture with ABCI blockchain consensus, gRPC API gateway, and modular state machine logic.

**Key Characteristics:**
- Tendermint/CometBFT consensus integration via ABCI protocol
- Framework-agnostic application state machine
- Pluggable storage backends (in-memory and RocksDB)
- gRPC-first API with REST gateway translation
- CosmWasm smart contract virtual machine integration
- Tracing/telemetry with OpenTelemetry and Jaeger

## Layers

**ABCI Application Server (Consensus):**
- Purpose: Connects to CometBFT consensus engine via ABCI protocol, processes blocks and transactions
- Location: `packages/abci/src/`
- Contains: Application trait, server implementation, codec for proto encoding/decoding
- Depends on: Tendermint protobuf definitions
- Used by: `Pulsarium` application wrapper

**Pulsarium (ABCI Implementation):**
- Purpose: Main blockchain application that wraps the state machine for ABCI compatibility
- Location: `app/slay3rd/src/app.rs`
- Contains: Request/response handling for ABCI lifecycle (init, check_tx, finalize_block, query)
- Depends on: `layer_app::App`, `layer_storage::PersistentStorage`
- Used by: CometBFT via ABCI socket connection

**Application State Machine (Core Logic):**
- Purpose: Framework-agnostic logic for processing transactions and queries
- Location: `packages/app/src/`
- Contains: `App` struct that manages blockchain state transitions, `StateMachine` with modules (auth, bank, wasm)
- Depends on: Storage interface, CosmWasm VM, utility packages
- Used by: Pulsarium wrapper, integration tests

**State Machine Modules:**
- **Auth:** `packages/app/src/auth/` - Validates transaction signatures, manages nonces, parses key formats
- **Bank:** `packages/app/src/bank/` - Manages token balances, handles transfers, native currency operations
- **Wasm:** `packages/app/src/wasm/` - CosmWasm VM execution, contract instantiation, message routing

**Storage Layer:**
- Purpose: Persistence abstraction with multiple implementations
- Location: `packages/storage/src/`
- Contains: `PersistentStorage` trait, `MemoryStore`, `RockStore` (RocksDB), prefixed storage, atomic transactions
- Depends on: None (fundamental trait definitions)
- Used by: `App` struct for all state access

**gRPC Services (Cosmos API):**
- Purpose: Cosmos-SDK compatible gRPC endpoints for blockchain queries and transactions
- Location: `app/slay3rd/src/grpc/`
- Contains: Service implementations for auth, bank, tx, tendermint, cosmwasm, sync
- Depends on: Query dispatcher from ABCI server, gRPC/tonic framework
- Used by: REST gateway, direct gRPC clients

**REST Gateway:**
- Purpose: Translates HTTP/REST to gRPC, provides JSON API
- Location: `gateway/` (Go application)
- Contains: gRPC mux, HTTP handlers, protobuf JSON marshaling
- Depends on: gRPC services running on port 9090
- Used by: External clients via HTTP on port 1317

**Supporting Packages:**
- **Proto:** `packages/proto/src/` - Protocol buffer definitions for all message types
- **Std:** `packages/std/src/` - Standard types (Msg, Query, Response, Block, Account)
- **Cosmos:** `packages/cosmos/src/` - Cosmos SDK type compatibility
- **Golem:** `packages/golem/src/` - Utility functions

## Data Flow

**Transaction Processing:**

1. Client sends transaction (binary or JSON) to gateway (HTTP) or gRPC endpoint
2. Gateway translates HTTP request to gRPC `SendTx` call → `app/slay3rd/src/grpc/tx.rs`
3. gRPC service forwards to ABCI query path
4. Pulsarium receives in ABCI `CheckTx` (mempool validation):
   - Validates signature via `Auth` module
   - Checks nonce and account existence
   - Simulates execution to verify validity
   - Returns mempool acceptance
5. CometBFT includes transaction in block
6. Pulsarium receives `FinalizeBlock`:
   - Executes each transaction against `App` state machine
   - Auth module verifies signature
   - Routes message to Bank, Wasm, or custom handler
   - Writes state changes to `Storage`
   - Collects events and gas usage
7. CometBFT commits block
8. Pulsarium handles `Commit`:
   - Persists final state to Storage
   - Returns app hash for block validation

**Query Processing:**

1. Client sends query (JSON-RPC or gRPC) to gateway
2. Gateway translates to gRPC service call → `app/slay3rd/src/grpc/*.rs`
3. gRPC service translates to ABCI `Query` path
4. Pulsarium dispatches through `StateMachine`:
   - Auth module: account balance, sequence numbers
   - Bank module: balances, denoms, supply
   - Wasm module: smart contract state, contract list
5. Result serialized as protobuf and returned through API

**State Management:**

- State machine maintains `App<T: PersistentStorage>` wrapped in `Arc<RwLock<>>`
- Each module namespaces storage with prefix keys
- Reads use `ReadonlyStorage` trait (cheap clones)
- Writes use `Storage` trait with atomic transactions
- On finalize_block: changes staged in `ScratchTx`, then committed

## Key Abstractions

**PersistentStorage Trait:**
- Purpose: Abstract storage backend (memory, RocksDB, future implementations)
- Examples: `MemoryStore`, `RockStore` in `packages/storage/src/`
- Pattern: Reader/writer pattern with atomic transactions via `ScratchTx`

**Msg/Query Union Types:**
- Purpose: Route incoming messages to correct handler
- Examples: In `packages/std/src/api/` - `MsgSend`, `MsgExecuteContract`
- Pattern: Serde-serializable enums, matched in state machine `execute()` method

**Module Interface:**
- Purpose: Each module (Auth, Bank, Wasm) implements consistent interface
- Pattern: `init()`, `query()`, `execute()` methods taking storage, gas meter, and request

**Storage Namespacing:**
- Purpose: Isolate module data within single key-value store
- Pattern: `prefixed()` wrapper creates isolated storage view for each module

## Entry Points

**ABCI Socket Server:**
- Location: `packages/abci/src/server.rs`
- Triggers: CometBFT connects on port specified in config
- Responsibilities: Listen for ABCI requests, dispatch to `Application` trait

**Pulsarium ABCI Application:**
- Location: `app/slay3rd/src/app.rs`
- Triggers: ABCI server calls trait methods (echo, info, check_tx, finalize_block, query)
- Responsibilities: Coordinate between ABCI protocol and `App` state machine

**gRPC Server:**
- Location: `app/slay3rd/src/main.rs` (setup), `app/slay3rd/src/grpc/` (handlers)
- Triggers: Network connections on gRPC port (9090)
- Responsibilities: Translate gRPC requests to ABCI queries, return results

**REST Gateway:**
- Location: `gateway/main.go`
- Triggers: HTTP client requests on REST port (1317)
- Responsibilities: Translate HTTP/JSON to gRPC, use gRPC-gateway to auto-marshal

**Main Daemon:**
- Location: `app/slay3rd/src/main.rs`
- Triggers: Binary execution with CLI args or environment variables
- Responsibilities: Parse config, initialize storage, start ABCI and gRPC servers

## Error Handling

**Strategy:** Result-based error handling with custom error types per layer.

**Patterns:**
- ABCI layer: `AbciError` from `packages/abci/src/error.rs` - protocol-level errors
- App layer: `PulsarError` from `packages/app/src/error.rs` - application logic errors
- Storage layer: `PlusError` from `packages/storage/src/plus.rs` - storage operation errors
- gRPC layer: `tonic::Status` - protocol buffer service errors
- Errors propagate upward with context, final response includes code and message

## Cross-Cutting Concerns

**Logging:** Structured logging via `tracing` crate with span context propagation. Configured in main.rs to output formatted text or forward to Jaeger.

**Validation:**
- Signatures validated in Auth module during CheckTx and FinalizeBlock
- Account existence/balance checked before state transitions
- Gas metering enforced at state machine level

**Authentication:**
- Ed25519/Secp256k1 public key formats supported
- Signature verification via `cosmwasm_crypto`
- Transaction must be signed; verification happens in Auth module

**Gas Metering:**
- All operations tracked via `GasMeter` passed through state transitions
- Per-byte costs: `GAS_COST_TX_BYTE = 10`
- Module operations (auth check, bank transfer, wasm execute) have fixed costs
- Query operations limited by `DEFAULT_QUERY_GAS = 500_000`

---

*Architecture analysis: 2026-03-18*
