# Layer SDK

## What This Is

Layer is a **meta-chain**: a stateful computational engine that exists both on and off-chain simultaneously. It can read from any blockchain, execute transactions on any network, and maintain its own cryptographically-secured state via EigenLayer restaking. WAVS programs run off-chain; Layer is the stateful on-chain backbone they read from and write to. The project is being revitalized after ~2 years of dormancy with a full stack replacement: Tendermint → Commonware consensus, CosmWasm → Ewasm (Ethereum types), and new bidirectional WAVS state integration.

## Core Value

WAVS programs can read from and write to persistent Layer state — enabling AVS operators to submit results on-chain and for Layer to serve as the stateful backbone of the EigenLayer meta-chain ecosystem, with state anchored to Ethereum via zkVM proofs.

## Requirements

### Validated

- ✓ ABCI application server with CosmWasm contract execution — existing
- ✓ Auth, Bank, and Wasm modules with state management — existing
- ✓ RocksDB-backed persistent storage with MemoryStore fallback — existing
- ✓ gRPC service layer (Cosmos SDK compatible) — existing
- ✓ REST gateway (Go) — existing
- ✓ Docker-based local node setup — existing
- ✓ gRPC concurrency safety: Arc<RwLock<App<T>>> — concurrent reads, exclusive writes for finalize_block (Phase 02.2)
- ✓ Graceful error handling for unhandled CosmosMsg variants (Stargate, Any, BankMsg, WasmMsg) — no node crashes (Phase 02.2)

### Active

- [ ] Replace Tendermint/CometBFT with Commonware consensus
- [ ] Fork CosmWasm as a git submodule (for custom host functions and Ewasm migration)
- [ ] Replace CosmWasm types with Ethereum types (Ewasm runtime)
- [ ] Support new WASM contracts written in Rust/AssemblyScript using Ethereum ABI encoding
- [ ] Bidirectional WAVS integration: AVS operators write state to Layer; Layer state is readable by WAVS programs
- [ ] zkVM state rollup to Ethereum via wreth node (zkVM TBD during research)

### Out of Scope

- CosmJS / Cosmos SDK client compatibility — switching to Ethereum types breaks this intentionally
- Tendermint P2P gossip protocol — replaced by Commonware networking
- Solidity-to-WASM as primary contract target — secondary, not required for v1
- Sharding / state partitioning — single-chain first

## Context

- Codebase is ~2 years stale. CometBFT, CosmWasm, and Tendermint crates will have had major version changes.
- Current stack: CometBFT v0.38.12, CosmWasm 1.5.4, Tendermint 0.39.1, RocksDB 0.22.0
- Unsafe lifetime management in WASM VM (`danger_will_robinson`) is a known footgun that may need to be addressed during the Ewasm migration
- Deterministic address generation for WASM instantiate is currently broken — must be fixed as part of Ewasm migration
- The `slay3rd` binary is the main daemon; `packages/app` is the framework-agnostic core
- WAVS integration builds on the existing `wavs` MCP skill available in this environment
- Commonware is a new consensus library — architecture decisions require the Commonware skill during research
- wreth is the target node for Ethereum state rollup; zkVM choice is undecided and must be researched

## Constraints

- **Consensus**: Must switch to Commonware — no new Tendermint dependencies
- **Types**: Ewasm runtime must use Ethereum address format (20 bytes) and ABI encoding, not Cosmos types
- **Submodule**: CosmWasm fork must be a git submodule (not a vendored copy)
- **WAVS compatibility**: WAVS integration must align with the wavs MCP service contract
- **Rollup**: zkVM must support Rust guest programs (required for wreth integration)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Fork CosmWasm as submodule | Need custom host functions AND migration bridge to Ewasm | — Pending |
| Commonware model TBD | Architecture depends on Commonware's primitives — research required | — Pending |
| Ewasm-first (Rust/AssemblyScript contracts) | Primary target; Solidity→WASM is secondary | — Pending |
| WAVS state is bidirectional | AVS operators write to Layer; Layer state readable by WAVS | — Pending |
| zkVM undecided | Must research SP1, Risc Zero, and wreth's native support | — Pending |

---
*Last updated: 2026-03-20 after Phase 02.2 — node stability fixes complete*
