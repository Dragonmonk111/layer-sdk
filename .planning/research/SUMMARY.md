# Project Research Summary

**Project:** Layer SDK Revitalization
**Domain:** Rust blockchain — Commonware consensus, Ewasm runtime, WAVS AVS state integration, zkVM rollup
**Researched:** 2026-03-18
**Confidence:** MEDIUM (Commonware ALPHA, WAVS pre-release crates, wreth architecture undocumented)

## Executive Summary

Layer is a Rust blockchain that requires four simultaneous major replacements: CometBFT consensus replaced with Commonware simplex, CosmWasm VM replaced with an Ewasm-compatible WASM runtime, a bidirectional WAVS state integration added for AVS operator persistence, and an optional zkVM state rollup to Ethereum via SP1. The project already has working RocksDB storage, Auth/Bank/Wasm module structure, and secp256k1 signing — these are valid foundations. What is being replaced is the type system (Cosmos bech32 → Ethereum 20-byte addresses), the consensus boundary (ABCI interface → Commonware Automaton trait), and the WASM execution environment (CosmWasm JSON + Wasmer → EEI host functions + wasmtime). The recommended approach is to build in five dependency-ordered phases starting with type foundation, working through consensus and runtime, and deferring zkVM proving to last as it requires all other pieces to be stable first.

The core value proposition is defensible and well-reasoned: WASM contracts with Ethereum types (not bytecode) gives Layer performance without EVM overhead, WAVS integration provides first-class persistent AVS state (something WAVS explicitly lacks today), and Commonware consensus delivers 200ms block times versus Tendermint's 1s. The recommended stack centers on `alloy-primitives` 1.5.7 as the canonical Ethereum type layer throughout, `wasmtime` 42.x replacing Wasmer in the CosmWasm fork, `commonware-consensus` 2026.3.0 replacing the ABCI stack, and SP1 v6.x for eventual zkVM proving. All four replacements are architecturally cohesive: they all move in the direction of Ethereum type compatibility, which is exactly what WAVS components expect.

The primary risks are concentrated in three areas. First, Commonware is ALPHA software — its API may change between minor versions, and it requires the application to implement its own mempool and block format (nothing is provided). Second, the address type migration from bech32 to H160 will silently corrupt all existing RocksDB state unless an explicit key re-encoding migration is written and tested. Third, the existing `danger_will_robinson` unsafe transmute in the WASM VM backend is incompatible with the async execution model Commonware introduces and must be eliminated before the migration begins. All three risks have clear mitigations; the danger is treating them as implementation details rather than first-class prerequisites.

---

## Key Findings

### Recommended Stack

See `.planning/research/STACK.md` for full details. The recommended stack replaces Cosmos-specific dependencies with Ethereum-compatible ones across every layer. `alloy-primitives` 1.5.7 is the universal Ethereum type layer — it replaces `cosmwasm-std` Addr (bech32) and aligns with revm, reth, WAVS, and SP1's own type dependencies. The CosmWasm fork should branch from v2.3.2 (not v1.5.4, which is the current version, and not v3.0 which adds irrelevant IBCv2 scope). Wasmer is replaced with `wasmtime` 42.x at the fork point, which provides cleaner deterministic fuel metering via `Config::consume_fuel()`. WAVS crates (`wavs-types` 0.3.0-alpha5) are pre-release and must be verified against `Lay3rLabs/awesome-WAVS` before pinning.

**Core technologies:**
- `commonware-consensus` 2026.3.0: BFT consensus via `simplex` algorithm — only purpose-built Rust-native BFT library; not a framework, you compose it
- `commonware-p2p` 2026.3.0: Authenticated peer networking — replaces Tendermint P2P entirely; must use authenticated transport or safety guarantees break
- `alloy-primitives` 1.5.7: Ethereum types (Address, U256, B256) — canonical choice used by revm/reth/WAVS; replaces all Cosmos address/amount types
- `wasmtime` 42.0.1: WASM execution engine — replaces Wasmer in CosmWasm fork; cleaner fuel metering API, Bytecode Alliance maintained
- `sp1-sdk` 6.0.2: zkVM proving (host-side) — fastest production zkVM with std Rust guest support, Groth16 Ethereum verifier; deferred to v2+
- `wavs-types` 0.3.0-alpha5: WAVS component ABI — required for implementing AVS operator result submission; pre-release, verify version

### Expected Features

See `.planning/research/FEATURES.md` for full details with dependency graph. The feature set is unusually well-constrained: every P1 feature is load-bearing for basic chain operation, there are no discretionary table-stakes features.

**Must have (v1 — table stakes):**
- Commonware Automaton trait implementation — the chain is dead without it; replaces all of ABCI
- Ethereum 20-byte address system — foundation for Ewasm and WAVS; requires storage key migration
- Ethereum ABI encoding — replaces CosmWasm JSON encoding; required for WAVS contract compatibility
- Ewasm EEI host functions (storage + execution context) — minimum viable contract execution
- Gas metering (instruction-level) — safety requirement; single contract can halt chain without it
- Deterministic CREATE2-style contract address derivation — fixes known bug at `keeper.rs:1001`
- WAVS `handleSignedEnvelope` submission interface — core AVS integration point on Layer
- AVS operator state write path — the entire value proposition depends on this
- Merkle state root / app hash — required for Commonware block validity
- secp256k1 signing (Ethereum-style, EIP-191/EIP-712) — wallet compatibility requirement

**Should have (v1.x — competitive):**
- Bidirectional WAVS state (Layer as persistent AVS state store) — primary differentiator; WAVS has no native persistent state
- Ethereum JSON-RPC endpoint — operator and client interface; enables standard tooling
- Block-level historical state queries — required for WAVS deterministic reads at specific block heights
- EEI inter-contract calls (`call`, `callDelegate`, `callStatic`, `create`) — contract composability
- Fine-grained contract permissions (partial privileges for promoted contracts)

**Defer (v2+):**
- zkVM state rollup to Ethereum via SP1 — requires all v1 features stable; very high complexity
- `threshold_simplex` consensus upgrade — enables succinct certificates for lite clients; defer until `simplex` is proven
- AssemblyScript contract toolchain — secondary contract language; defer until Rust Ewasm SDK is stable
- Solidity-to-WASM compilation — immature toolchains; not the primary target

**Anti-features (deliberately not building):**
- Cosmos SDK / CosmJS compatibility — maintaining two type systems is the explicit anti-goal
- Full EVM bytecode execution (zkEVM) — categorically different from Ewasm; doubles runtime complexity
- Sharding — premature; Layer's value is purpose-built WAVS state store, not general throughput

### Architecture Approach

See `.planning/research/ARCHITECTURE.md` for full diagrams and component breakdown. The architecture has a clean layered structure: Commonware consensus at the top communicates with the application only through the `Automaton` trait (propose/verify/finalization notification); the application (`App<T>`) holds Auth, Bank, and Ewasm modules backed by `PersistentStorage`; the Ewasm module embeds `wasmtime` in-process with EEI host functions as Rust closures; WAVS is external and writes to Layer via standard Ewasm contract transactions. The key architectural insight is that the storage layer (`packages/storage/`) is already the right abstraction and does not need to change — EEI 256-bit slot reads/writes translate cleanly to the existing prefixed key-value store. The ABCI layer (`packages/abci/`) is a complete replacement, not a migration.

**Major components:**
1. `LayerNode` (Automaton impl) — replaces `Pulsarium`; drives Commonware consensus; manages application-owned mempool; communicates with `App<T>` via `Arc<RwLock>`
2. `packages/ewasm/` (new) — EEI host function implementations; `eei.rs` defines the ~40 standard functions; `context.rs` holds per-call execution state; `meter.rs` handles gas accounting
3. `packages/app/src/wasm/vm/` (replaced) — wasmtime linker setup; replaces the Wasmer-based CosmWasm VM; keeps keeper structure and cache module
4. AVS contracts (new Ewasm contracts) — `avs-tasks/`, `avs-verifier/`, `operator-registry/` replace the current `contracts/` directory; written in Rust compiled to WASM with EEI imports
5. `packages/storage/` (preserved) — `RockStore` and `MemoryStore`; unchanged; everything maps through `PersistentStorage` trait
6. zkVM guest program (deferred) — Rust program compiled to RISC-V ELF; must contain full state transition logic (all keeper modules); stateless re-execution from witness

**Key patterns:**
- Automaton as consensus bridge: `propose()` / `verify()` replace ABCI `prepare_proposal` / `process_proposal`; finalization is push-based notification
- EEI as storage bridge: synchronous in-process host functions; no IPC, no sidecar process
- WAVS writes as standard Layer transactions: AVS operator results arrive as normal txs targeting the Ewasm verifier contract; no special consensus path
- Merkle state commitment is a hard prerequisite for zkVM (current RocksDB storage produces no state roots)

### Critical Pitfalls

See `.planning/research/PITFALLS.md` for full details including recovery strategies and phase-to-pitfall mapping.

1. **Commonware non-determinism halts consensus** — Any `HashMap`/`HashSet` iteration, `SystemTime::now()`, or float arithmetic in `certify()` or block validation causes nodes to diverge and stall. Prevention: replace all `HashMap` with `BTreeMap` in consensus-critical paths; establish a "determinism boundary" annotation; run determinism fuzz tests before multi-node testnet.

2. **Address type migration silently corrupts RocksDB state** — Changing `AccountId` from bech32 to H160 changes all storage key bytes; existing accounts, balances, and contracts become invisible. Prevention: write an explicit atomic `WriteBatch` migration per keeper before any type alias changes go live; test with pre-seeded bech32 state.

3. **Unsafe WASM VM transmute incompatible with Commonware async executor** — `danger_will_robinson` in `wasm/vm/backend.rs` bypasses lifetime checks; adding Commonware's async task scheduler creates new code paths that can violate the transmute's narrow safety invariants. Prevention: run `cargo miri test` on all VM tests; refactor before Ewasm migration begins; this is a prerequisite, not a parallel workstream.

4. **Non-deterministic contract address generation (existing bug)** — `keeper.rs:1001` is explicitly marked non-deterministic; different nodes will have different contract registries. Prevention: fix to `keccak256(deployer || tx_hash || nonce)` before first Commonware testnet. This is a consensus correctness blocker.

5. **zkVM guest/host boundary breaks proof security** — If the state transition function runs on the host and only a pre-computed hash is passed to the guest, the Groth16 proof is cryptographically valid but provides zero security. Prevention: all keeper/module logic must execute inside the guest; design the proof boundary in the architecture phase before writing any zkVM code.

6. **CosmWasm gas mispricing (CWA-2024-004) carried into fork** — CosmWasm 1.5.4 (current version) is in the affected range for a gas metering vulnerability allowing ~10x computation per gas unit. Prevention: cherry-pick the fix from CosmWasm 1.5.7 on day one of the fork; add CI to track upstream security advisories.

7. **Cargo dependency conflicts from 2-year dormancy** — Pinned `prost`, `bytes`, `tokio` versions from 2023 will conflict with Commonware, WAVS, and SP1 transitive dependencies. Prevention: run `cargo update` on a fresh branch before adding new dependencies; run `cargo tree -d` after each addition; resolve `prost` conflicts first as they surface as type-level errors.

---

## Implications for Roadmap

Based on combined research, the phase structure is driven by hard dependency ordering: Ethereum types must exist before Ewasm; Ewasm must work before WAVS contracts can be deployed; Commonware can run in parallel with Ewasm development but requires the state machine to exist; zkVM requires a Merkle state root which is a significant storage addition.

### Phase 1: Foundation — Ethereum Types and Dependency Hygiene

**Rationale:** All subsequent work depends on Ethereum address types being canonical. This phase has zero external dependencies and can be validated entirely with `MemoryStore`. The dependency audit must happen here before anything else is added — fixing Cargo conflicts early is far cheaper than untangling them mid-migration. The `danger_will_robinson` unsafe audit and the contract address generation bug fix belong here because they are prerequisites for the Commonware phase.
**Delivers:** Clean Ethereum address types throughout `packages/std/`, `packages/app/src/auth/`, `packages/app/src/bank/`; resolved Cargo dependency tree; deterministic contract address generation; `danger_will_robinson` documented and refactored; storage key migration tests scaffolded.
**Addresses:** Ethereum 20-byte address system, secp256k1 Ethereum-style signing, deterministic CREATE2-style address derivation, nonce/sequence tracking preservation
**Avoids:** Address type migration corruption (Pitfall 3), unsafe WASM VM transmute incompatibility (Pitfall 2), non-deterministic address generation (Pitfall 4), Cargo dependency conflicts (Pitfall 7)

### Phase 2: Commonware Consensus Integration

**Rationale:** Consensus replacement is architecturally independent from the VM replacement — `LayerNode` implementing `Automaton` only needs the state machine's `execute()` path to exist, not the WASM subsystem specifically. Running Commonware before the full Ewasm migration validates the consensus boundary early and allows parallel Ewasm work. Determinism tests must be part of this phase's acceptance criteria.
**Delivers:** Working `LayerNode` implementing `Automaton`; application-owned mempool; `Relay` implementation for Commonware broadcast; `packages/abci/` deleted; multi-node testnet reaching consensus with identical AppHash across nodes; WAL sync configuration validated; crash-recovery test passing.
**Uses:** `commonware-consensus` 2026.3.0, `commonware-p2p` 2026.3.0, `commonware-runtime` 2026.3.0, `commonware-cryptography` 2026.3.0, `commonware-storage` 2026.3.0
**Implements:** `LayerNode` (Automaton), `Relay` trait, mempool
**Avoids:** Consensus non-determinism (Pitfall 1), WAL sync misconfiguration (Pitfall 5)

### Phase 3: Ewasm Runtime (CosmWasm Fork + wasmtime + EEI)

**Rationale:** The CosmWasm VM replacement is the highest-risk single migration in the project. It must come after Phase 1 (needs Ethereum types), can run in parallel with Phase 2 but is safer after consensus is validated. The security patch for CWA-2024-004 must be applied on day one. The fork should branch from v2.3.2, not the current v1.5.4.
**Delivers:** `packages/ewasm/` with full EEI host function set (storage, execution context, gas metering); `wasmtime` linker setup replacing Wasmer in CosmWasm fork; ABI encoding replacing JSON throughout; Ewasm versions of `root`, `caller`, and `echo` contracts; gas metering with patched constants from CosmWasm 1.5.7; WASM linear memory limits enforced per instance.
**Uses:** `wasmtime` 42.0.1, `alloy-primitives` 1.5.7, `alloy-sol-types` 1.5.x, CosmWasm fork at v2.3.2, `revm` 36.0.0 (for EVM precompile host functions)
**Implements:** Ewasm module, EEI host functions, gas metering
**Avoids:** CosmWasm gas mispricing CWA-2024-004 (Pitfall 8), in-process execution (avoids sidecar anti-pattern), WASM linear memory exhaustion (performance trap)

### Phase 4: WAVS Bidirectional State Integration

**Rationale:** WAVS integration requires working Ewasm contracts — the AVS task, verifier, and operator registry contracts must execute on Layer. This phase delivers the project's primary differentiator: persistent AVS state backed by a blockchain. Historical state query support (block-level snapshots) is a v1.x addition but should be designed in this phase.
**Delivers:** AVS task contract (Ewasm), verifier contract (Ewasm), operator registry contract (Ewasm); WAVS trigger configuration; operator state write path via `handleSignedEnvelope`; replay protection on WAVS writes (sequence numbers); Ethereum JSON-RPC endpoint for operator tx submission; Layer query interface usable by WAVS components.
**Uses:** `wavs-types` 0.3.0-alpha5, `wavs-wasi-chain` 0.3.0, `alloy-sol-types` 1.5.x
**Implements:** WAVS bidirectional state (both directions), AVS contracts
**Avoids:** WAVS write replay attacks (integration gotcha), RocksDB polling anti-pattern for WAVS reads, operator-local mutable state anti-feature

### Phase 5: zkVM State Rollup to Ethereum (v2+)

**Rationale:** This phase has the highest complexity and the most unknowns. It requires: a Merkle state root (currently absent from storage), finalized blocks from Commonware, a stable Ewasm runtime, and clarity on the "wreth" architecture. The wreth project is not publicly documented and requires an internal architecture decision before SP1 integration can begin. Deferring to v2+ is correct — attempting this in parallel with any other phase would derail the entire project.
**Delivers:** Merkle state root (sparse Merkle tree or MPT) added to storage layer; execution witness export; SP1 guest program containing full Layer state transition function; STARK-to-Groth16 compression; Ethereum on-chain verifier contract; state root update flow.
**Uses:** `sp1-sdk` 6.0.2, `sp1-zkvm` 6.0.2, `sp1-build` 6.0.2
**Implements:** zkVM rollup layer, wreth integration
**Avoids:** zkVM guest/host boundary security failure (Pitfall 6), proving without state roots (anti-pattern 3)

### Phase Ordering Rationale

- **Phase 1 before everything:** Ethereum types are not just a refactor — they change storage key encodings. Doing this first means all subsequent work builds on the correct foundation. The dependency audit and unsafe VM audit belong here because they are prerequisites, not implementation tasks.
- **Phase 2 (Consensus) before Phase 4 (WAVS) but can parallel Phase 3:** Commonware consensus validates the block finalization model that WAVS state writes depend on. Ewasm runtime work is largely independent — it can proceed in parallel with Phase 2, but Phase 3 should not be declared complete until it runs under Commonware consensus (not just unit tests).
- **Phase 3 before Phase 4:** WAVS contracts are Ewasm contracts. The Ewasm runtime must work before WAVS integration can be validated end-to-end.
- **Phase 5 last:** Hard dependency on Phases 1-4 being stable. Merkle state root is a significant addition to the storage layer that affects everything above it — adding it during earlier phases creates unnecessary instability.
- **The wreth architecture decision must happen during Phase 4 planning:** By the time Phase 4 is complete, wreth's design must be clarified internally so Phase 5 design can proceed with confidence.

### Research Flags

Phases likely needing deeper research (`/gsd:research-phase`) during planning:
- **Phase 2 (Commonware):** Commonware is ALPHA; the finalization callback mechanism is not fully documented in public API docs; the `Supervisor` trait for validator set management has no existing implementation in the codebase; mempool design is entirely application-defined with no framework guidance
- **Phase 3 (Ewasm):** The CosmWasm v2.x `BackendApi` replacement (address canonicalization → Ethereum semantics) needs a detailed implementation plan; wasmtime linker setup for the full ~40 EEI function set needs API research; wasmtime + CosmWasm ABI compatibility for existing contracts needs validation
- **Phase 5 (zkVM):** wreth architecture is undocumented publicly; SP1 guest std support with the state transition function needs verification (rocksdb/tonic must be excluded from guest); Merkle state commitment choice (MPT vs sparse Merkle tree) requires research

Phases with standard patterns (can skip research-phase or limit scope):
- **Phase 1 (Type Foundation):** Address type migration is well-understood; `alloy-primitives` API is stable and documented; Cargo dependency resolution is mechanical
- **Phase 4 (WAVS):** WAVS submission architecture is documented in official docs; the AVS contract pattern (task/verifier/operator registry) is already implemented in the commitments repo; WAVS trigger configuration is documented

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Core crate versions verified via docs.rs (HIGH); WAVS crate versions are pre-release (LOW); SP1/wreth integration details unverified (LOW); Commonware ALPHA (MEDIUM) |
| Features | MEDIUM | Table stakes are well-defined from EEI spec and WAVS docs (HIGH); differentiator features confirmed from WAVS design docs (MEDIUM); zkVM feature requirements inferred from SP1 patterns (MEDIUM) |
| Architecture | MEDIUM | Automaton trait signatures confirmed via docs.rs (HIGH); finalization callback mechanism not fully documented (MEDIUM); WAVS bidirectional specifics partially documented (MEDIUM); wreth architecture unknown (LOW) |
| Pitfalls | HIGH | Determinism pitfall documented in Commonware docs (HIGH); CWA-2024-004 has official advisory (HIGH); unsafe transmute identified via first-party codebase audit (HIGH); address migration corruption is a known pattern (HIGH) |

**Overall confidence:** MEDIUM

The stack, feature set, and architectural patterns are well enough understood to build a roadmap with high confidence through Phase 4. Phase 5 (zkVM/wreth) has LOW confidence on the wreth-specific integration details and requires an internal architecture decision before implementation planning can proceed meaningfully.

### Gaps to Address

- **wreth definition:** The term "wreth" appears in PROJECT.md but is not publicly indexed. It may be an internal Layer Labs project, a custom reth fork, or a shorthand for a different concept. This must be clarified before Phase 5 design can proceed. If it refers to a custom reth derivative, the SP1 + reth stateless execution pattern applies directly.
- **Commonware Supervisor trait:** The `Supervisor` trait controls the active validator set in Commonware. The existing codebase has no validator set management code at all. This needs design work before Phase 2 can be completed — who can add/remove validators, and how does this interact with WAVS operator registration?
- **Merkle state root choice:** The current RocksDB storage layer produces no state roots. Phase 5 (and to some extent Phase 2's app hash) requires a Merkle commitment over state. The choice between a Merkle Patricia Trie (Ethereum-compatible, more complex) and a sparse Merkle tree (simpler, sufficient for Layer's use case) should be decided during Phase 2 planning.
- **WAVS submission contract on-chain format:** With Ethereum type migration, the `IWavsServiceHandler` interface requires `handleSignedEnvelope()` to accept ABI-encoded payloads. The exact ABI schema for this function, and how it interacts with the AVS verifier contract on Layer, needs to be specified before Phase 4 implementation begins.
- **Existing contract migration path:** The existing contracts (`root`, `caller`, `echo`) are CosmWasm contracts using JSON encoding and Cosmos types. They must be rewritten as Ewasm contracts. A migration path document should be produced during Phase 3 to communicate breaking changes to any downstream users.

---

## Sources

### Primary (HIGH confidence)
- [commonware-consensus 2026.3.0 — docs.rs](https://docs.rs/commonware-consensus/latest/commonware_consensus/) — Automaton, Relay, Reporter trait definitions; version confirmed
- [wasmtime 42.0.1 — docs.rs](https://docs.rs/wasmtime/latest/wasmtime/) — fuel API, Linker API, host function registration
- [alloy-primitives 1.5.7 — docs.rs](https://docs.rs/crate/alloy-primitives/latest) — version confirmed; canonical Ethereum types
- [sp1-sdk 6.0.2 — docs.rs](https://docs.rs/crate/sp1-sdk/latest) — version confirmed; host-side proving API
- [CosmWasm advisories CWA-2024-004](https://github.com/CosmWasm/advisories/blob/main/CWAs/CWA-2024-004.md) — gas mispricing vulnerability in 1.5.4
- [CosmWasm releases — GitHub](https://github.com/CosmWasm/cosmwasm/releases) — v2.x and v3.x release history; fork point rationale
- [Ewasm EEI specification](https://ewasm.readthedocs.io/en/mkdocs/eth_interface/) — complete ~40 EEI host function list
- [WAVS how it works — docs.wavs.xyz](https://docs.wavs.xyz/how-it-works) — handleSignedEnvelope, IWavsServiceHandler interface
- Layer SDK existing codebase (`packages/`, `contracts/`, `app/`) — first-party audit; keeper patterns, storage abstractions, existing bugs

### Secondary (MEDIUM confidence)
- [Commonware Anti-Framework blog](https://commonware.xyz/blogs/commonware-the-anti-framework) — application boundary philosophy; no prescribed block format
- [SP1 Hypercube mainnet — Succinct blog](https://blog.succinct.xyz/sp1-hypercube-is-now-live-on-mainnet/) — real-time Ethereum proving performance (self-reported)
- [WAVS custom components — docs.wavs.xyz](https://docs.wavs.xyz/handbook/components/component) — WasmResponse, TriggerAction, Guest trait
- [WAVS design considerations — docs.wavs.xyz](https://docs.wavs.xyz/design) — persistent operator-local state unsupported; deterministic execution requirements
- [Alto blockchain reference — GitHub](https://github.com/commonwarexyz/alto) — minimal Commonware blockchain example; finalization pattern
- [Lay3rLabs/awesome-WAVS — GitHub](https://github.com/Lay3rLabs/awesome-WAVS) — WAVS ecosystem crates and examples
- [risc0-zkvm 3.0.5 — docs.rs](https://docs.rs/risc0-zkvm/3.0.5/risc0_zkvm/) — fallback zkVM comparison
- Layer commitments repo (`/Users/jacobhartnell/Dev/projects/Layer/commitments/`) — AVS task/verifier/operator contract architecture

### Tertiary (LOW confidence)
- [SP1 vs Risc Zero comparison — Medium](https://medium.com/@gwrx2005/comparative-analysis-of-sp1-and-risc-zero-zero-knowledge-virtual-machines-4abf806daa70) — single analysis source; precompile extensibility comparison
- [wavs-wasi-chain 0.3.0 — docs.rs](https://docs.rs/crate/wavs-wasi-chain/latest) — pre-release; version may change
- [WAVS on Layer announcement](https://www.layer.xyz/news-and-insights/introducing-wavs-the-next-gen-avs-builder) — bidirectional state intent; marketing document

---
*Research completed: 2026-03-18*
*Ready for roadmap: yes*
