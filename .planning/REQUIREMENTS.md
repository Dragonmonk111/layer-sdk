# Requirements: Layer SDK

**Defined:** 2026-03-18
**Core Value:** WAVS programs can read from and write to persistent Layer state — enabling Layer to serve as the stateful backbone of the EigenLayer meta-chain ecosystem, with state anchored to Ethereum via zkVM proofs.

> **Note:** No backwards compatibility required. Tendermint/CometBFT and CosmJS dependencies are to be removed. Leverage existing Rust Ethereum ecosystem libraries (`alloy`, `revm`, `wasmtime`) wherever possible.

## v1 Requirements

### Foundation

- [x] **FOUND-01**: Codebase builds cleanly with 2026 dependencies — all Tendermint/CometBFT crates removed, dependency conflicts resolved, `cargo audit` passes
- [ ] **FOUND-02**: `danger_will_robinson` unsafe lifetime transmute in `packages/app/src/wasm/vm/` resolved before Commonware async contexts are added
- [ ] **FOUND-03**: Non-deterministic WASM contract address generation (`keeper.rs:1001`) fixed

### CosmWasm Fork

- [ ] **FORK-01**: CosmWasm forked at v2.3.2 as a git submodule in this repo (not a vendored copy; fork must include cherry-pick of CWA-2024-004 gas mispricing security fix as day-one work)
- [ ] **FORK-02**: Custom host function injection mechanism in place — `BackendApi` trait and linker infrastructure to register Ethereum-specific host functions

### Commonware Consensus

- [ ] **CONS-01**: CometBFT ABCI server (`packages/abci/`, `app/slay3rd/`) replaced with Commonware `threshold_simplex` Automaton implementation
- [ ] **CONS-02**: `propose()`, `verify()`, and `genesis()` Automaton callbacks wired to the existing `App<T>` state machine
- [ ] **CONS-03**: Validator set management implemented via Commonware `Supervisor` trait — initial static validator set, extensible for future dynamic management
- [ ] **CONS-04**: State transitions are fully deterministic — no `HashMap` iteration, no `SystemTime`, no floats in any code path reachable from `certify()` or `verify()`
- [ ] **CONS-05**: Consensus produces threshold signature certificates (BLS12-381) per block — prerequisite for zkVM rollup proofs

### Ethereum Types

- [ ] **TYPES-01**: Cosmos bech32 `Addr` type replaced with `alloy_primitives::Address` (20-byte EIP-55) throughout all packages — auth, bank, wasm keeper, proto, and grpc layers
- [ ] **TYPES-02**: RocksDB storage key re-encoding migration — existing keyed data re-encoded to new address format without silent data loss
- [ ] **TYPES-03**: Transaction signing updated to Ethereum ECDSA (secp256k1 + keccak256 message hashing) using `alloy-signer` or equivalent
- [ ] **TYPES-04**: Transaction format uses Ethereum ABI encoding (`alloy-abi` / `ethabi`) instead of Cosmos protobuf amino/proto encoding

### WASM Runtime (Ethereum Types)

- [ ] **WASM-01**: CosmWasm VM backend replaced with a de-Cosmos'd WASM runtime implemented on wasmtime — all Cosmos-specific types stripped from `packages/app/src/wasm/vm/` and replaced with Ethereum types
- [ ] **WASM-02**: Ethereum-style host functions implemented: `storageLoad`, `storageStore`, `getCaller`, `getCallValue`, `getBlockNumber`, `getBlockTimestamp`
- [ ] **WASM-03**: Ethereum-style execution functions implemented: `useGas`, `finish`, `revert`, `log0`–`log4`
- [ ] **WASM-04**: CREATE2-style deterministic contract address generation (keccak256(deployer + salt + initcode hash)) — replaces the broken non-deterministic instantiate
- [ ] **WASM-05**: Gas metering via wasmtime `consume_fuel` — deterministic, configurable, no hardcoded limits
- [ ] **WASM-06**: Rust/AssemblyScript contracts using Ethereum ABI encoding can be deployed and executed end-to-end on a local node

### WAVS Integration

- [ ] **WAVS-01**: AVS task queue contract deployable as a WASM contract on Layer — accepts task submissions from any caller
- [ ] **WAVS-02**: WAVS operators can submit signed results to Layer via a `handleSignedEnvelope`-equivalent on-chain interface
- [ ] **WAVS-03**: Layer state is readable by WAVS programs via the gRPC query interface (read path confirmed working with a reference WAVS component)
- [ ] **WAVS-04**: Operator registry WASM contract on Layer tracks authorized AVS operators
- [ ] **WAVS-05**: WAVS components can trigger Layer transactions via the write path (full bidirectional state loop demonstrated end-to-end)

### zkVM Rollup

- [ ] **ZKVM-01**: Sparse Merkle tree (or MPT) state root computed per finalized block and stored in block header
- [ ] **ZKVM-02**: SP1 guest program re-executes Layer state transitions given a block's inputs — produces a valid STARK proof
- [ ] **ZKVM-03**: State proof submitted to an Ethereum verifier contract via wreth node integration
- [ ] **ZKVM-04**: wreth node runs alongside `slay3rd` — finalized Layer blocks trigger proof generation and Ethereum settlement

## v2 Requirements

### Ethereum Tooling Compatibility

- **COMPAT-01**: EVM-compatible JSON-RPC endpoint (`eth_call`, `eth_getBalance`, `eth_sendRawTransaction`) for use with standard Ethereum tooling
- **COMPAT-02**: Solidity→WASM contract support (compiled via Revive/resolc) — secondary contract target after Rust/AssemblyScript

### Performance

- **PERF-01**: Excessive `clone()` operations in hot execution paths profiled and reduced
- **PERF-02**: Configurable pagination in gRPC query endpoints (currently hardcoded at 100)

### Observability

- **OBS-01**: Build metadata (`build_tags`, `build_deps`) in version info endpoint
- **OBS-02**: Capabilities advertisement endpoint — clients can query which gRPC methods are supported

## Out of Scope

| Feature | Reason |
|---------|--------|
| Cosmos SDK client compatibility (CosmJS, Keplr, amino) | Intentionally broken by Ethereum type migration |
| IBC (Inter-Blockchain Communication) | Not needed for meta-chain model; adds Cosmos coupling |
| Backwards compatibility with existing Tendermint-keyed state | No migration path required; clean break |
| Tendermint P2P gossip protocol | Replaced by Commonware networking |
| CometBFT RPC endpoint | Replaced by Commonware; no longer needed |
| Mobile clients | Web3/Ethereum tooling sufficient for v1 |
| Sharding / state partitioning | Single-chain architecture for v1 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | Phase 1 | Complete |
| FOUND-02 | Phase 1 | Pending |
| FOUND-03 | Phase 1 | Pending |
| FORK-01 | Phase 1 | Pending |
| FORK-02 | Phase 1 | Pending |
| CONS-01 | Phase 2 | Pending |
| CONS-02 | Phase 2 | Pending |
| CONS-03 | Phase 2 | Pending |
| CONS-04 | Phase 2 | Pending |
| CONS-05 | Phase 2 | Pending |
| TYPES-01 | Phase 3 | Pending |
| TYPES-02 | Phase 3 | Pending |
| TYPES-03 | Phase 3 | Pending |
| TYPES-04 | Phase 3 | Pending |
| WASM-01 | Phase 4 | Pending |
| WASM-02 | Phase 4 | Pending |
| WASM-03 | Phase 4 | Pending |
| WASM-04 | Phase 4 | Pending |
| WASM-05 | Phase 4 | Pending |
| WASM-06 | Phase 4 | Pending |
| WAVS-01 | Phase 5 | Pending |
| WAVS-02 | Phase 5 | Pending |
| WAVS-03 | Phase 5 | Pending |
| WAVS-04 | Phase 5 | Pending |
| WAVS-05 | Phase 5 | Pending |
| ZKVM-01 | Phase 6 | Pending |
| ZKVM-02 | Phase 6 | Pending |
| ZKVM-03 | Phase 6 | Pending |
| ZKVM-04 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 29 total
- Mapped to phases: 29
- Unmapped: 0

---
*Requirements defined: 2026-03-18*
*Last updated: 2026-03-18 — phase reorder (Commonware → Phase 2, Ethereum Types → Phase 3, WASM Runtime → Phase 4); "Ewasm" terminology replaced with "WASM Runtime (Ethereum Types)" / "de-Cosmos'd WASM runtime"*
