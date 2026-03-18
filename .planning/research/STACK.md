# Stack Research

**Domain:** Rust Blockchain — Consensus Replacement + Ewasm Runtime + WAVS Integration + zkVM Rollup
**Researched:** 2026-03-18
**Confidence:** MEDIUM (Commonware ALPHA, WAVS actively evolving; core crate versions verified via docs.rs)

---

## Executive Summary

This research covers five distinct technology dimensions needed for the Layer SDK revitalization:

1. **Commonware** — replacement consensus layer (currently CometBFT v0.38.12)
2. **Ewasm / WASM runtime** — replacement contract execution (currently CosmWasm 1.5.4 + Wasmer)
3. **WAVS** — EigenLayer AVS integration
4. **zkVM** — SP1 vs Risc Zero for Ethereum state rollup
5. **CosmWasm fork strategy** — where to diverge from upstream

The recommended positions: Commonware 2026.3.x as consensus layer using the `simplex` or `threshold_simplex` algorithm; `wasmtime` 42.x as the WASM engine for the Ewasm runtime (replacing the Wasmer dependency in CosmWasm VM); CosmWasm forked at `v2.x` (not `v3.0`) to retain Wasmer familiarity while adding Ethereum host functions; SP1 v6.x as the zkVM (fastest, most production-hardened in 2026, with revm/reth integration); WAVS via `wavs-types` + `wavs-wasi-chain` for the component ABI; and `alloy-primitives` 1.5.x as the canonical Ethereum type layer throughout.

---

## Recommended Stack

### 1. Consensus Layer — Commonware

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `commonware-consensus` | 2026.3.0 | BFT agreement, block ordering | The only purpose-built, primitives-first Rust consensus library designed for custom state machines; not a framework — you compose it |
| `commonware-p2p` | 2026.3.0 | Authenticated peer networking | Encrypted, identity-based P2P, designed to work directly under commonware-consensus |
| `commonware-cryptography` | 2026.3.0 | Key generation, signing, verification | Provides BLS12-381, Ed25519, secp256r1; threshold signature support for `threshold_simplex` |
| `commonware-runtime` | 2026.3.0 | Async task scheduling | Swappable between deterministic (test) and tokio (production); required by all other Commonware crates |
| `commonware-storage` | 2026.3.0 | Persistent state journal | Write-ahead log; pairs with RocksDB backend for durable block storage |

**Integration pattern — mandatory traits to implement:**

The core abstraction is the `Automaton` trait from `commonware-consensus`. Your application state machine must implement:

```rust
// Propose the next payload (transaction batch / state transition)
fn propose(&mut self, context: Context) -> impl Future<Output = Bytes>;

// Verify a received proposal before consensus commits to it
fn verify(&mut self, context: Context, payload: Bytes) -> impl Future<Output = bool>;

// Called after consensus finalizes a block; apply state transitions
fn notarized(&mut self, proof: Proof, payload: Bytes);
fn finalized(&mut self, proof: Proof, payload: Bytes);
```

The `simplex` module provides single-leader BFT (~200ms blocks, ~300ms finality at 150ms network latency). The `threshold_simplex` module adds threshold BLS12-381 signatures, enabling succinct consensus certificates for bridge and rollup use cases — directly relevant to the wreth/zkVM rollup goal.

**Confidence:** MEDIUM — Commonware is ALPHA (expect breaking changes between 2026.x minor versions). The Alto reference blockchain exists and the trait API is stable enough to build against, but pin exact versions in Cargo.lock.

---

### 2. WASM Runtime — Ewasm Execution Layer

The current CosmWasm VM uses Wasmer 5.0.6 as its WASM engine. For the Ewasm migration (replacing Cosmos types with Ethereum types), there are two sub-decisions: (a) which WASM engine, and (b) which Ethereum execution types.

#### 2a. WASM Engine

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `wasmtime` | 42.0.1 | WASM execution engine | Bytecode Alliance standard; Cranelift JIT; deterministic fuel metering; no GPL; mature gas metering API |

**Why wasmtime over wasmer for this project:**

- `cosmwasm-vm` currently uses Wasmer 5.0.6. For the Ewasm fork, switching to `wasmtime` is recommended because:
  - Wasmtime's `Config::consume_fuel()` provides deterministic, per-instruction fuel metering — required for blockchain gas accounting
  - Wasmtime's `Linker` with `func_wrap()` provides clean host function injection (this is the extension point for Ethereum host functions like `keccak256`, ABI encode/decode, EVM precompiles)
  - Wasmtime is maintained by the Bytecode Alliance with no licensing surprises
  - `cosmwasm-vm` is being forked anyway — switching engines at fork time is lower cost than later

**Key wasmtime patterns for blockchain WASM:**

```rust
// Gas metering
let mut config = Config::new();
config.consume_fuel(true);
let engine = Engine::new(&config)?;
let mut store = Store::new(&engine, state);
store.set_fuel(gas_limit)?;

// Custom host functions (Ethereum-specific imports)
let mut linker = Linker::new(&engine);
linker.func_wrap("env", "keccak256", |caller: Caller<_>, ptr: i32, len: i32| { ... })?;
linker.func_wrap("env", "abi_encode", |caller: Caller<_>, ...| { ... })?;
```

#### 2b. Ethereum Execution Types

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `alloy-primitives` | 1.5.7 | Address (20 bytes), U256, B256, FixedBytes | Canonical Ethereum primitive types; used by revm, reth, and all major 2025+ tooling |
| `alloy-sol-types` | ~1.5.x | ABI encoding/decoding for Solidity types | Required for Ethereum-typed contract interfaces; used by WAVS components |
| `revm` | 36.0.0 | EVM execution engine | NOT for WASM contract execution, but for EVM precompile implementation within the WASM host |

**On `revm` scope:** The project runs WASM contracts (Rust/AssemblyScript), not EVM bytecode. `revm` is NOT the primary contract execution engine. However, `revm` is the correct choice for implementing the EVM precompile host functions that contracts can call (ecrecover, sha3, identity, etc.), and it is what SP1/Zeth use for proving Ethereum state.

**What NOT to use:**
- The original Ewasm spec (`ewasm.readthedocs.io`) — this project was effectively abandoned by Ethereum Foundation in favor of other scaling approaches. The "Ewasm" name in this project refers to WASM contracts using Ethereum types, not the deprecated EF Ewasm specification.
- `polkavm` — Substrate-specific, not relevant here.
- Wasmer for the fork — you can keep it if you want to minimize diff against upstream CosmWasm, but fuel metering in Wasmer 5 is less ergonomic than wasmtime's fuel API.

---

### 3. CosmWasm Fork Strategy

| Decision | Recommendation | Rationale |
|----------|---------------|-----------|
| Fork version | `v2.x` (specifically `v2.3.2`) | v2.x dropped cosmwasm-storage, simplified gas, renamed BackendApi methods — cleaner foundation. v3.x added IBCv2 which is irrelevant and adds scope |
| Fork mechanism | Git submodule pointing to a Lay3rLabs fork | Required by PROJECT.md constraint; allows upstream patches to be cherry-picked |
| What to replace | CosmWasm types throughout, host functions, address format | Replace `cosmwasm-std` Addr (bech32) with `alloy_primitives::Address` (20-byte hex); replace `cosmwasm-std::Uint128` usage with `U256` where contract-facing |

**CosmWasm version history after 1.5.4:**

| Version | Key Change | Relevance to Fork |
|---------|------------|------------------|
| 2.0.x | Gas values reduced 1000x; `CosmosMsg::Stargate` → `Any`; `cosmwasm-storage` removed; `u128/i128` JSON changed | BREAKING — fork at or after 2.0 to avoid carrying this diff |
| 2.2.x | `addr_canonicalize`/`addr_humanize` renamed in `BackendApi`; `addr_validate` added | BackendApi is the exact trait you need to replace for Ethereum addresses |
| 2.3.x | Latest stable 2.x line (2.3.2 as of Feb 2025) | Recommended fork point |
| 3.0.x | IBCv2 entrypoints; new `cw-schema` format; partial reference types (Rust 1.86) | Skip — unnecessary scope for this project |

**BackendApi replacement (the critical change):**

In the forked `cosmwasm-vm`, the `BackendApi` trait controls address canonicalization. The fork must replace this with Ethereum address handling:

```rust
// Current cosmwasm BackendApi in v2.x:
pub trait BackendApi {
    fn addr_validate(&self, input: &str) -> BackendResult<()>;
    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>>;
    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String>;
}

// Fork target: replace with Ethereum address semantics
// addr_canonicalize: parse checksummed hex address → [u8; 20]
// addr_humanize: [u8; 20] → checksummed hex string (EIP-55)
```

**Confidence:** HIGH for fork strategy recommendation. MEDIUM for implementation complexity — the existing `danger_will_robinson` unsafe lifetime issue in WASM VM must be audited before shipping.

---

### 4. WAVS Integration

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `wavs-types` | 0.3.0-alpha5 | Core WAVS data structures (`TriggerAction`, `WasmResponse`) | Required for implementing WAVS component Guest trait |
| `wavs-wasi-chain` | 0.3.0 | Chain interaction helpers (alloy-based RPC) | Utilities for EVM event decoding and chain state queries inside WASI components |
| `wavs-wasi-utils` | latest | HTTP requests, ABI decoding, trigger data helpers | `decode_event_log_data!` macro; required for trigger parsing |
| `alloy-sol-types` | ~1.5.x | `sol!` macro for Solidity type generation | Used to define the on-chain contract interface that WAVS components submit results to |
| `wstd` | latest | Async `block_on` in WASI environments | Required for async operations inside WASI components |
| `wit-bindgen` | latest | WIT interface bindings | Auto-generates Rust bindings from the WAVS WIT interface |

**WAVS architecture for Layer SDK:**

The bidirectional integration has two distinct sides:

**Side A — AVS operators write to Layer (inbound):**
- WAVS component runs off-chain as a WASI module
- Operator processes EigenLayer trigger event
- Component returns `Option<WasmResponse>` with ABI-encoded payload
- WAVS runtime submits the result to an on-chain contract
- That on-chain contract (on Layer) receives the AVS result and stores it in Layer state

**Side B — Layer state readable by WAVS (outbound):**
- Layer exposes a gRPC/REST query endpoint
- WAVS components use `wavs-wasi-chain` with alloy provider to read Layer state
- This requires Layer to expose EVM-compatible JSON-RPC OR a custom `wavs-wasi-chain` provider

**Key constraint:** WAVS currently targets "EVM and Cosmos chains" for its submission layer. Layer SDK switching from Cosmos types to Ethereum types is the *right* move to align with WAVS — EVM-typed transactions and ABI-encoded state are exactly what WAVS components expect.

**Confidence:** LOW for exact WAVS crate versions — `wavs-types` is still `0.3.0-alpha5` (pre-release). Verify against `Lay3rLabs/awesome-WAVS` GitHub before pinning. The integration pattern (WIT interface + Guest trait + WasmResponse) is MEDIUM confidence from official docs.

---

### 5. zkVM — SP1 vs Risc Zero Decision

#### Recommendation: SP1 v6.x

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `sp1-sdk` | 6.0.2 | SP1 host-side proving SDK | Proves arbitrary Rust programs; fastest production zkVM in 2026 |
| `sp1-zkvm` | 6.0.2 | SP1 guest-side crate (in zkVM) | Use inside programs that run inside the zkVM guest |
| `sp1-build` | 6.0.2 | Build script helper for guest ELF | Compile guest Rust to RISC-V ELF at build time |

**Why SP1 over Risc Zero:**

| Criterion | SP1 v6 | Risc Zero v3 |
|-----------|--------|-------------|
| Proving speed | 99.7% of ETH blocks <12s on 16× RTX 5090 | ~20s as of June 2025 |
| Precompile extensibility | First-class; open constraint logic | Limited — constraint logic closed-source |
| Compiler optimization impact | More consistent gains | Degradation 4× more common than SP1 |
| Production usage (2026) | Optimism, Arbitrum, Polygon, Mantle ($2B TVL), Celo | Boundless network (launched July 2025); fewer major rollup adoptions |
| Guest language | Standard Rust with std | no_std Rust; more restrictive |
| Ethereum block proving | SP1 Hypercube; real-time proven | Zeth/Reth integration; ~20s blocks |
| Open source | 100% open source (MIT/Apache) | Partially closed constraint logic |

**Key SP1 usage pattern for Layer state rollup:**

```toml
# Host (prover) Cargo.toml
[dependencies]
sp1-sdk = "6.0.2"

# Guest Cargo.toml (runs inside SP1)
[dependencies]
sp1-zkvm = { version = "6.0.2", features = [] }
alloy-primitives = { version = "1.5.7", default-features = false }
```

```rust
// Host: prove a Layer state transition
use sp1_sdk::{ProverClient, SP1Stdin};

let client = ProverClient::new();
let (pk, vk) = client.setup(LAYER_STATE_ELF);
let mut stdin = SP1Stdin::new();
stdin.write(&layer_state_root);
stdin.write(&transactions);
let proof = client.prove(&pk, stdin).run()?;
```

**For wreth integration:** The referenced "wreth" is not a well-established public project as of this research date. It may refer to an internal Layer Labs concept. SP1 is the correct choice because:
1. SP1's `rsp` (Rust State Prover) proves Ethereum block execution using reth's stateless execution
2. SP1 Hypercube is already proving live Ethereum blocks in real-time
3. The pattern (SP1 guest + reth stateless execution) maps directly to proving Layer state transitions rolled up to Ethereum

**If wreth/Risc Zero is mandated:** Use `risc0-zkvm` 3.0.5 + `risc0-steel` for Ethereum state access. Risc Zero's `Zeth` project (Type-0 zkEVM using reth) is the equivalent of SP1's rsp. The integration is structurally identical but currently slower.

**Confidence:** MEDIUM — SP1 performance numbers are from official Succinct blog posts (HIGH confidence for relative comparison), but exact wreth integration details cannot be verified (LOW confidence). Phase-specific research on wreth required.

---

## Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `alloy-primitives` | 1.5.7 | Ethereum types (Address, U256, B256) | Everywhere — replace all Cosmos address/amount types |
| `alloy-sol-types` | ~1.5.x | ABI encode/decode, `sol!` macro | Contract interfaces, WAVS submission encoding |
| `revm` | 36.0.0 | EVM precompile implementations | Implementing EVM precompiles as WASM host functions |
| `tokio` | 1.x | Async runtime | Keep existing; Commonware runtime wraps tokio in production |
| `tonic` | 0.12.x | gRPC | Keep for existing gRPC layer; evaluate if Commonware P2P replaces |
| `tracing` | 0.1.x | Structured logging | Keep; compatible with all new crates |
| `rocksdb` | 0.22.x → latest | Persistent storage | Upgrade from 0.22.0; works with commonware-storage |
| `serde` | 1.x | Serialization | Keep; all new crates use serde |
| `thiserror` | 2.x | Error types | Upgrade from 1.x; no API changes |

---

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Consensus | Commonware 2026.3.x | CometBFT 0.38.x | PROJECT.md constraint: "no new Tendermint dependencies"; also Tendermint is Go-based and adds FFI boundary |
| Consensus | Commonware simplex | LibP2P + custom BFT | Massive scope increase; Commonware already integrates P2P + BFT |
| WASM engine | wasmtime 42.x | Wasmer 5.x (current) | Wasmer's fuel API is less ergonomic; wasmtime has better determinism guarantees for blockchain use |
| WASM engine | wasmtime 42.x | wasmi | wasmi is interpreter-only — too slow for production smart contracts |
| Ethereum types | alloy-primitives | ethereum-types (0.14.x) | `ethereum-types` is a legacy crate; `alloy-primitives` is the 2025+ canonical choice used by revm/reth |
| zkVM | SP1 v6 | Risc Zero v3 | SP1 is faster, more open, broader production adoption in 2026 rollup stacks |
| zkVM | SP1 v6 | OpenVM / ZisK | Both are newer entrants in the Ethereum L1-zkEVM space; less ecosystem maturity for custom state machines |
| CosmWasm fork base | v2.3.2 | v3.0.4 | v3 adds IBCv2 (irrelevant), new schema format adds scope; v2.x is a cleaner, smaller fork surface |
| CosmWasm fork base | v2.3.2 | v1.5.4 (current) | v1.5.x carries two years of upstream fixes and security patches; staying on 1.5.4 is technical debt |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| CometBFT / Tendermint | PROJECT.md hard constraint; adds Go runtime dependency; fundamentally incompatible with Commonware P2P | `commonware-consensus` + `commonware-p2p` |
| `cosmwasm-std` types for addresses | Bech32 addresses are Cosmos-specific; breaks WAVS and Ethereum compatibility | `alloy_primitives::Address` (20-byte EIP-55) |
| `cosmrs` | Cosmos transaction builder — wrong types after Ethereum migration | `alloy` transaction types |
| `@cosmjs/*` JS libraries | Cosmos-specific; will be fully incompatible after Ethereum type migration | `viem` or `ethers.js` v6 for JS client |
| `tendermint-rpc` / `tendermint-proto` | Remove entirely; Commonware has its own P2P/RPC layer | Commonware networking primitives |
| Ewasm EF spec (ewasm.readthedocs.io) | EF's original Ewasm project was abandoned; do not implement the metering injection spec | Custom wasmtime fuel metering with alloy types |
| `polkavm` | Substrate-specific RISC-V VM; no Ethereum ecosystem integration | `wasmtime` for WASM; SP1/revm for ZK/EVM |
| `opentelemetry-jaeger` | The Jaeger exporter crate was deprecated; `opentelemetry-jaeger` 0.18 is unmaintained | `opentelemetry-otlp` with OTLP gRPC exporter |

---

## Installation

```toml
# Consensus layer
[dependencies]
commonware-consensus = "2026.3.0"
commonware-p2p       = "2026.3.0"
commonware-cryptography = "2026.3.0"
commonware-runtime   = "2026.3.0"
commonware-storage   = "2026.3.0"

# Ethereum types (replaces Cosmos types throughout)
alloy-primitives = { version = "1.5.7", default-features = false, features = ["std"] }
alloy-sol-types  = { version = "~1.5", default-features = false, features = ["std"] }

# WASM execution engine (replaces Wasmer in CosmWasm fork)
wasmtime = { version = "42.0.1", default-features = false, features = ["cranelift", "async", "fuel"] }

# EVM precompiles (for WASM host functions)
revm = { version = "36.0.0", default-features = false, features = ["std"] }

# WAVS integration
wavs-types       = "0.3.0-alpha5"   # verify against Lay3rLabs/wavs before pinning
wavs-wasi-chain  = "0.3.0"
wavs-wasi-utils  = "*"              # pin once stable version confirmed

# zkVM (host-side, proving infrastructure)
sp1-sdk = "6.0.2"

# Guest crate (used inside SP1 RISC-V programs only)
[target.'cfg(target_arch = "riscv32")'.dependencies]
sp1-zkvm = { version = "6.0.2", default-features = false }
```

---

## Version Compatibility Notes

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| commonware-* 2026.3.0 | tokio 1.x | Commonware runtime wraps tokio; no tokio version conflict |
| wasmtime 42.x | Rust 1.80+ | MSRV raised significantly; verify Rust toolchain version |
| alloy-primitives 1.5.7 | revm 36.x | Both alloy v1.x compatible; co-developed by same team |
| sp1-sdk 6.0.2 | Rust stable | SP1 guest requires nightly for some features; check sp1 toolchain |
| cosmwasm-vm 2.3.x (fork base) | wasmer 5.0.6 | Fork will replace wasmer with wasmtime; this is a non-trivial refactor |
| risc0-zkvm 3.0.5 | NOT concurrent with sp1-sdk | Do not add both zkVM crates to the same crate; choose one |

---

## Stack Patterns by Phase

**If building the Commonware consensus integration first:**
- Implement `Automaton` trait against your existing `packages/app` state machine
- Use `commonware-runtime::tokio::Runner` in production, `::deterministic::Runner` in tests
- The existing ABCI server interface will be replaced by the Automaton trait; the gRPC layer (tonic) remains

**If building the Ewasm runtime first:**
- Fork CosmWasm at v2.3.2 as a git submodule into `vendor/cosmwasm`
- Replace `cosmwasm-vm/src/backend.rs` BackendApi with Ethereum address semantics
- Replace `wasmer` dependency with `wasmtime`; re-implement the host imports in `wasmtime`'s `Linker` API
- Add `alloy-primitives` to `cosmwasm-std` fork for `Addr` replacement

**If building WAVS integration first:**
- Start with an EVM-compatible trigger contract on Layer (using alloy types)
- Implement WAVS component using `wavs-types` Guest trait
- The bidirectional state access requires Layer to expose EVM-compatible RPC

**For SP1 rollup integration:**
- The zkVM guest must be able to re-execute Layer's state transition function
- This requires `packages/app` state transition logic to be `no_std` compatible OR carefully gated
- SP1's standard library support means full `std` Rust is possible in the guest — this is a significant advantage over Risc Zero

---

## Confidence Assessment

| Area | Confidence | Basis |
|------|------------|-------|
| Commonware crate versions | HIGH | Verified via docs.rs: `2026.3.0` confirmed |
| Commonware integration pattern | MEDIUM | Derived from official Commonware blog + DeepWiki documentation; ALPHA status means API may shift |
| wasmtime as Ewasm engine | MEDIUM | Version confirmed (42.0.1); fuel API confirmed; replacing Wasmer in CosmWasm fork is a known-complex refactor |
| alloy-primitives as type layer | HIGH | v1.5.7 confirmed; canonical choice for revm/reth/WAVS ecosystem |
| CosmWasm fork at v2.3.2 | HIGH | Version confirmed; BackendApi trait analysis confirmed; IBCv2 scope argument is solid |
| SP1 as zkVM | MEDIUM | v6.0.2 confirmed; performance claims from official Succinct blog; wreth-specific integration details unverified |
| Risc Zero as fallback | MEDIUM | v3.0.5 confirmed; Zeth/reth integration confirmed; performance numbers from official RISC Zero blog |
| WAVS crate versions | LOW | wavs-types is alpha5; versions may change; only integration pattern (not versions) is HIGH confidence |

---

## Open Questions (for Phase Research)

1. **wreth node definition** — "wreth" is referenced in PROJECT.md but does not appear in public repositories. Is this an internal Lay3rLabs project, or a typo for reth? Must be clarified before the zkVM rollup phase begins. If it means a custom reth fork, the SP1 + reth integration pattern applies directly.

2. **Commonware Supervisor trait** — who maintains the validator set? The `Supervisor` trait controls active participants. The existing codebase has no validator-set management; this needs to be designed before Commonware consensus can be integrated.

3. **WAVS chain-side submission contract** — Layer needs a Solidity-compatible (or EVM ABI-compatible) contract to receive WAVS results. With the Ethereum type migration, what does this contract look like, and how does it interact with Layer's WASM module system?

4. **wasmtime + CosmWasm ABI compatibility** — the CosmWasm 2.x contract ABI (memory layout, exports like `allocate`/`deallocate`, the JSON-over-memory encoding) must remain compatible after the wasmtime migration. Existing contracts compiled against cosmwasm-std 1.x should still execute; this is a non-trivial compatibility constraint.

5. **SP1 guest std support for Layer state machine** — SP1 supports std Rust, but verify that the `rocksdb` and `tonic` dependencies can be excluded from the guest program. The guest only needs the pure state transition logic, not storage or networking.

---

## Sources

- [commonware-consensus 2026.3.0 — docs.rs](https://docs.rs/commonware-consensus/latest/commonware_consensus/) — version, traits, modules (HIGH confidence)
- [commonware-p2p 2026.3.0 — docs.rs](https://docs.rs/commonware-p2p/latest/commonware_p2p/) — version, API surface (HIGH confidence)
- [commonware-runtime 2026.3.0 — docs.rs](https://docs.rs/crate/commonware-runtime/latest) — version confirmed (HIGH confidence)
- [Commonware Anti-Framework Philosophy — DeepWiki](https://deepwiki.com/commonwarexyz/monorepo/1.1-anti-framework-philosophy) — CertifiableAutomaton, Runner, Signer traits (MEDIUM confidence)
- [commonwarexyz/monorepo — GitHub](https://github.com/commonwarexyz/monorepo) — Alto example, crate list (MEDIUM confidence)
- [Commonware alto benchmark — commonware.xyz](https://alto.commonware.xyz/) — performance numbers, simplex consensus (MEDIUM confidence)
- [wasmtime 42.0.1 — docs.rs](https://docs.rs/wasmtime/latest/wasmtime/) — version, fuel API, host function API (HIGH confidence)
- [revm 36.0.0 — docs.rs](https://docs.rs/revm/latest/revm/) — version, EVM execution traits (HIGH confidence)
- [cosmwasm-vm 3.0.4 — docs.rs](https://docs.rs/cosmwasm-vm/latest/cosmwasm_vm/) — version, Wasmer 5.0.6 engine, BackendApi trait (HIGH confidence)
- [CosmWasm releases — GitHub](https://github.com/CosmWasm/cosmwasm/releases) — v2.x and v3.x release history (HIGH confidence)
- [CosmWasm 3.0 — Medium](https://medium.com/cosmwasm/cosmwasm-3-0-fd84d72c2d35) — IBCv2, cw-schema, breaking changes (MEDIUM confidence)
- [alloy-primitives 1.5.7 — docs.rs](https://docs.rs/crate/alloy-primitives/latest) — version confirmed (HIGH confidence)
- [sp1-sdk 6.0.2 — docs.rs](https://docs.rs/crate/sp1-sdk/latest) — version confirmed (HIGH confidence)
- [SP1 Hypercube mainnet — Succinct blog](https://blog.succinct.xyz/sp1-hypercube-is-now-live-on-mainnet/) — real-time Ethereum proving performance (MEDIUM confidence — self-reported)
- [risc0-zkvm 3.0.5 — docs.rs](https://docs.rs/risc0-zkvm/3.0.5/risc0_zkvm/) — version, Receipt types, prover API (HIGH confidence)
- [risc0/zeth — GitHub](https://github.com/risc0/zeth) — Ethereum block proving with reth (MEDIUM confidence)
- [SP1 vs Risc Zero comparison — Medium/@gwrx2005](https://medium.com/@gwrx2005/comparative-analysis-of-sp1-and-risc-zero-zero-knowledge-virtual-machines-4abf806daa70) — precompile extensibility, compiler optimization analysis (LOW confidence — single analysis source)
- [wavs-wasi-chain 0.3.0 — docs.rs](https://docs.rs/crate/wavs-wasi-chain/latest) — version, alloy-based chain helpers (MEDIUM confidence)
- [WAVS custom components — docs.wavs.xyz](https://docs.wavs.xyz/handbook/components/component) — WasmResponse, TriggerAction, Guest trait, wavs-types version (MEDIUM confidence)
- [Lay3rLabs/awesome-WAVS — GitHub](https://github.com/Lay3rLabs/awesome-WAVS) — WAVS ecosystem crates, integration examples (MEDIUM confidence)

---
*Stack research for: Layer SDK — Consensus + Ewasm + WAVS + zkVM*
*Researched: 2026-03-18*
