# Roadmap: Layer SDK

## Overview

Layer is a meta-chain being revitalized from a 2-year dormant state. The journey moves in strict dependency order: first clean the foundation (unsafe VM, broken addresses, stale dependencies) and fork CosmWasm; then replace CometBFT with Commonware consensus and its threshold signature certificates (while the state machine still uses Cosmos types); then migrate the entire type system to Ethereum addresses and ABI encoding; then replace the CosmWasm VM with a de-Cosmos'd WASM runtime on wasmtime using Ethereum-style host functions; then wire in the bidirectional WAVS state integration that is the project's primary value; and finally anchor Layer state to Ethereum via SP1 zkVM proofs. Each phase delivers a coherent, independently verifiable capability.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Foundation** - Clean the codebase, fix the unsafe VM transmute, resolve 2-year-old dependency conflicts, and fork CosmWasm as a git submodule (completed 2026-03-18)
- [x] **Phase 2: Commonware Consensus** - Replace CometBFT ABCI with Commonware simplex Automaton — multi-node testnet reaches consensus with BLS threshold certificates (completed 2026-03-19)
- [x] **Phase 2.1: Functional Node** (INSERTED) - Real cross-process P2P, gRPC interface, tx pipeline, RocksDB persistence (gap closure in progress) (completed 2026-03-20)
- [ ] **Phase 3: Ethereum Types** - Replace Cosmos bech32 addresses with Ethereum 20-byte addresses and ABI encoding throughout every package
- [ ] **Phase 4: WASM Runtime (Ethereum Types)** - Replace the CosmWasm VM with a de-Cosmos'd WASM runtime on wasmtime — Rust contracts using Ethereum-style host functions can deploy and execute end-to-end
- [ ] **Phase 5: WAVS Integration** - Deploy AVS contracts on Layer and demonstrate the full bidirectional state loop with a WAVS component
- [ ] **Phase 6: zkVM Rollup** - Compute Merkle state roots per block and submit SP1 STARK proofs to an Ethereum verifier via wreth

## Phase Details

### Phase 1: Foundation
**Goal**: The codebase builds cleanly against 2026 dependencies, the unsafe WASM VM transmute is eliminated, contract address generation is deterministic, and CosmWasm is forked as a git submodule with the CWA-2024-004 security fix applied
**Depends on**: Nothing (first phase)
**Requirements**: FOUND-01, FOUND-02, FOUND-03, FORK-01, FORK-02
**Success Criteria** (what must be TRUE):
  1. `cargo build` and `cargo test` complete without errors; `cargo audit` passes with no critical advisories; no CometBFT or Tendermint crates remain in the dependency tree
  2. `danger_will_robinson` unsafe lifetime transmute in `packages/app/src/wasm/vm/` is replaced with a safe alternative and confirmed by `cargo miri test` passing on all VM tests
  3. Contract instantiation produces the same address on every node given the same deployer and salt (deterministic, no `SystemTime` or random input)
  4. CosmWasm is present as a git submodule at `v2.3.2` with CWA-2024-004 cherry-picked; the fork builds and the `BackendApi` trait is extensible for custom host functions
**Plans**: 3 plans

Plans:
- [x] 01-01-PLAN.md — Delete ABCI/slay3rd, remove Tendermint workspace deps, verify clean build
- [x] 01-02-PLAN.md — Add CosmWasm fork submodule, upgrade BackendApi v1 to v2, add Ethereum stubs
- [x] 01-03-PLAN.md — Eliminate unsafe transmute, add address determinism regression test

### Phase 2: Commonware Consensus
**Goal**: CometBFT ABCI is removed and replaced by a Commonware `threshold_simplex` Automaton; a multi-node local testnet reaches consensus, produces identical AppHash across all nodes, and generates BLS12-381 threshold signature certificates per finalized block — the state machine may still use Cosmos types at this stage
**Depends on**: Phase 1 (FOUND-02 prerequisite for safe async contexts)
**Requirements**: CONS-01, CONS-02, CONS-03, CONS-04, CONS-05
**Success Criteria** (what must be TRUE):
  1. `packages/abci/` is deleted; the `slay3rd` binary starts using `LayerNode` implementing the Commonware `Automaton` trait with no CometBFT or Tendermint imports anywhere in the workspace
  2. A 3-node local testnet reaches consensus and all nodes produce the same AppHash for the same finalized block (determinism validated — no `HashMap` iteration, `SystemTime`, or floats in consensus-critical paths)
  3. A node that crashes and restarts from its WAL rejoins the testnet and catches up to the current block without manual intervention
  4. Each finalized block carries a BLS12-381 threshold signature certificate produced by Commonware `threshold_simplex`; the certificate is stored in the block header and verifiable by an offline verifier
**Plans**: 5 plans

Plans:
- [x] 02-01-PLAN.md — Create slay3rd crate with Commonware deps, determinism audit (HashMap -> BTreeMap)
- [x] 02-02-PLAN.md — Implement LayerNode CertifiableAutomaton, BlockPayload, Mempool
- [x] 02-03-PLAN.md — BLS DKG keygen tool, node config, P2P relay, main.rs consensus runtime
- [x] 02-04-PLAN.md — 3-node testnet scripts, crash recovery test, BLS certificate verification
- [x] 02-05-PLAN.md — Gap closure: wire BLS certificate from Reporter to persistent storage (CONS-05)

### Phase 02.1: Functional Node (INSERTED)

**Goal:** Deliver a fully functional Layer node with real cross-process authenticated P2P consensus, a tonic gRPC interface (Cosmos queries, BroadcastTx, state sync streaming), a working transaction pipeline (submit via gRPC to mempool to block to CosmWasm execute), and RocksDB persistence — the minimum viable baseline before the Ethereum type migration begins
**Requirements**: None (gap-closure phase addressing P2P transport, gRPC server, tx pipeline, RocksDB persistence)
**Depends on:** Phase 2
**Success Criteria** (what must be TRUE):
  1. 3 real OS processes communicate via `commonware_p2p::authenticated::lookup` and reach consensus with identical AppHash
  2. A tonic gRPC server on each node serves Cosmos SDK queries (bank, auth, wasm), BroadcastTx (sync mode), and state sync streaming
  3. Transactions submitted via gRPC BroadcastTx pass check_tx, enter the mempool, get included in blocks, and execute CosmWasm messages
  4. State persists across node restarts via RocksDB (`RockStore` replacing `MemoryStore`)
  5. End-to-end: deploy contracts/root/ contract (StoreCode + InstantiateContract + ExecuteContract msgs) via gRPC, query resulting state
**Plans:** 4/4 plans complete

Plans:
- [x] 02.1-01-PLAN.md — Config + dependencies + gRPC service module
- [x] 02.1-02-PLAN.md — main.rs integration (authenticated P2P, RocksDB, gRPC wiring, tx pipeline)
- [x] 02.1-03-PLAN.md — Testnet scripts + Docker Compose + E2E verification
- [ ] 02.1-04-PLAN.md — Gap closure: Cosmos query dispatch + tx-sender tool + full e2e contract deployment

### Phase 3: Ethereum Types
**Goal**: Every package uses `alloy_primitives::Address` (20-byte EIP-55) instead of Cosmos bech32 `Addr`; transactions are signed and encoded in Ethereum format; RocksDB storage keys are re-encoded without data loss
**Depends on**: Phase 2.1 (functional node baseline required before type migration)
**Requirements**: TYPES-01, TYPES-02, TYPES-03, TYPES-04
**Success Criteria** (what must be TRUE):
  1. A 20-byte Ethereum address passes through the auth, bank, wasm keeper, proto, and gRPC layers without bech32 encoding or decoding at any point
  2. Existing RocksDB test fixtures with bech32 keys can be migrated atomically to H160 keys and read back with no data loss (key re-encoding migration passes with pre-seeded state)
  3. A transaction signed with an Ethereum ECDSA private key (secp256k1 + keccak256) is accepted and executed by the node
  4. Transaction payloads are ABI-encoded and ABI-decoded correctly through the full tx processing path (no protobuf or amino encoding remaining for user-facing transactions)
**Plans**: TBD

### Phase 4: WASM Runtime (Ethereum Types)
**Goal**: The CosmWasm VM backend is fully replaced by a de-Cosmos'd WASM runtime on wasmtime; all Cosmos-specific types (bech32 addresses, CosmMsg, CosmosSDK encoding) are stripped from the VM layer and replaced with Ethereum types (alloy Address, ABI encoding, keccak256); Rust contracts using idiomatic Ethereum-style host functions can be deployed and executed end-to-end on a local node
**Depends on**: Phase 3 (Ethereum types migration must complete before replacing the VM layer)
**Requirements**: WASM-01, WASM-02, WASM-03, WASM-04, WASM-05, WASM-06
**Success Criteria** (what must be TRUE):
  1. A Rust contract compiled to WASM can be deployed to a local node using CREATE2-style address derivation and produce the same contract address on repeated deployment attempts with the same parameters
  2. A deployed contract can read from and write to persistent storage using idiomatic Ethereum-style host functions (`storageLoad` / `storageStore`); state persists across block boundaries
  3. Gas is consumed per instruction via wasmtime `consume_fuel`; a contract that exceeds its gas limit is reverted cleanly without halting the node; gas limits are configurable
  4. The `root`, `caller`, and `echo` contracts (rewritten for the new runtime) pass their integration tests against the de-Cosmos'd WASM runtime using Ethereum ABI encoding
**Plans**: TBD

### Phase 5: WAVS Integration
**Goal**: AVS task queue, verifier, and operator registry WASM contracts are deployed on Layer; WAVS operators can submit signed results to Layer; Layer state is readable by WAVS programs; the full bidirectional state loop is demonstrated end-to-end
**Depends on**: Phase 4 (WASM runtime with Ethereum types), Phase 2 (Commonware consensus and finalized blocks)
**Requirements**: WAVS-01, WAVS-02, WAVS-03, WAVS-04, WAVS-05
**Success Criteria** (what must be TRUE):
  1. An AVS task queue WASM contract deployed on Layer accepts task submissions from any caller and stores them in contract storage retrievable via the gRPC query interface
  2. A WAVS operator submits a signed result to Layer via `handleSignedEnvelope`; the verifier contract validates the signature, updates on-chain state, and the operator registry confirms the operator is authorized
  3. A WAVS component reads current Layer state via the gRPC query interface and receives a correct, finalized response (read path confirmed working with a reference WAVS component against a live local node)
  4. A WAVS component triggers a Layer transaction via the write path; the transaction is included in a finalized block; the updated state is readable back through the gRPC query interface (full bidirectional loop demonstrated end-to-end)
**Plans**: TBD

### Phase 6: zkVM Rollup
**Goal**: Each finalized Layer block carries a Sparse Merkle Tree state root; an SP1 guest program re-executes Layer state transitions to produce a STARK proof; proofs are submitted to an Ethereum verifier contract via wreth node integration
**Depends on**: Phase 2 (CONS-05 threshold certificates), Phase 5 (stable state from all preceding phases)
**Requirements**: ZKVM-01, ZKVM-02, ZKVM-03, ZKVM-04
**Success Criteria** (what must be TRUE):
  1. Every finalized Layer block header contains a Sparse Merkle Tree (or MPT) state root computed from the post-execution state; the root changes when state changes and is identical across all nodes
  2. An SP1 guest program re-executes a Layer block's state transitions given the block inputs and produces a valid STARK proof that matches the state root in the block header
  3. A finalized Layer block's state proof is submitted to and accepted by an Ethereum verifier contract deployed on a test network (Groth16 proof verification passes on-chain)
  4. A wreth node running alongside `slay3rd` automatically triggers proof generation after each block finalization and submits the resulting proof to Ethereum without manual intervention
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 2.1 -> 3 -> 4 -> 5 -> 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation | 3/3 | Complete   | 2026-03-18 |
| 2. Commonware Consensus | 5/5 | Complete   | 2026-03-19 |
| 2.1 Functional Node | 4/4 | Complete   | 2026-03-20 |
| 3. Ethereum Types | 0/TBD | Not started | - |
| 4. WASM Runtime (Ethereum Types) | 0/TBD | Not started | - |
| 5. WAVS Integration | 0/TBD | Not started | - |
| 6. zkVM Rollup | 0/TBD | Not started | - |
