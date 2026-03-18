# Feature Research

**Domain:** WAVS-integrated Ethereum-compatible blockchain (Layer SDK revitalization)
**Researched:** 2026-03-18
**Confidence:** MEDIUM — Commonware is ALPHA software with sparse docs; WAVS on-chain state model is partially documented; Ewasm EEI is well-specified but the original ewasm project is orphaned and the approach is being adapted

---

## Research Context

Layer is a Rust blockchain being revitalized with four simultaneous replacements:
1. Consensus: Tendermint/CometBFT → Commonware (simplex)
2. Runtime: CosmWasm → Ewasm (Ethereum type system)
3. Integration: New — bidirectional WAVS state (AVS operators write; WAVS components read)
4. Rollup: New — zkVM state proof submission to Ethereum via wreth node

The existing codebase has Auth, Bank, and Wasm modules backed by RocksDB. AccountId currently supports both 20-byte (Ethereum) and 32-byte addresses with bech32 encoding. Transaction signing supports secp256k1; Ed25519 is stubbed but not implemented.

---

## Feature Landscape

### Table Stakes (System Does Not Work Without These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Commonware Automaton trait implementation | Without this there is no consensus; the entire chain is dead | HIGH | Must implement `genesis()`, `propose()`, `verify()`, and optionally `certify()` for CertifiableAutomaton. No ABCI equivalent — block format is application-defined. Commonware is ALPHA; API may shift. |
| Ethereum 20-byte address system (AccountId) | Ewasm runtime requires Ethereum address semantics; WAVS submits results to Ethereum-typed contracts | MEDIUM | AccountId already supports 20-byte addresses. Must drop the 32-byte path and bech32 encoding in favor of hex checksummed addresses (EIP-55) or raw 0x-prefixed bytes. |
| Ethereum ABI encoding for contract I/O | Ewasm contracts encode/decode using Ethereum ABI, not JSON. WAVS components consume Ethereum ABI output. | HIGH | Replace cosmwasm_std JSON encoding with ethabi or alloy-sol-types. All existing contracts break; migration path required. |
| Ewasm host functions (EEI) — storage I/O | Contracts cannot read or write persistent state without `storageLoad`/`storageStore` host functions | HIGH | Must expose: `storageLoad(pathOffset, resultOffset)`, `storageStore(pathOffset, valueOffset)` mapping to the chain's key-value store. Uses 32-byte keys and 32-byte values (EVM slot model). |
| Ewasm host functions (EEI) — execution context | Contracts cannot know who called them or what block they're in without block/tx context host functions | MEDIUM | Must expose: `getCaller`, `getCallValue`, `getBlockNumber`, `getBlockTimestamp`, `getBlockGasLimit`, `getGasLeft`, `useGas`, `finish`, `revert`. |
| Gas metering (instruction-level) | Without per-instruction gas, a contract can loop forever and halt the chain | HIGH | Ewasm specifies gas as a 64-bit integer with 4-decimal-digit precision ("particles"). Must inject metering into WASM bytecode at compile time (metering injection) or at runtime via the host. Existing `SDK_TO_WASMER_GAS_FACTOR` conversion approach must be redesigned. |
| secp256k1 transaction signing (Ethereum-style) | Users expect to sign transactions with MetaMask/standard Ethereum wallets | MEDIUM | Current secp256k1 verification works but uses Cosmos SDK signing format. Must migrate to Ethereum signing (EIP-191 personal sign or EIP-712 typed data). Ed25519 stub can be removed. |
| Nonce/sequence tracking per address | Replay protection; without nonces, the same transaction can be submitted multiple times | LOW | Already implemented in Auth module. Must be preserved through migration. |
| Persistent key-value storage with namespacing | Every module needs isolated storage; contracts need isolated storage slots | LOW | Already implemented via prefixed storage + RocksDB. Preserve this; adapt key format if needed for Ethereum slot semantics. |
| Deterministic contract address derivation | Without deterministic addresses, transactions cannot be replayed and consensus breaks. Currently broken (FIXME in keeper.rs:1001). | HIGH | Must fix `build_instantiate_address` using CREATE2-equivalent: `keccak256(deployer_address ++ salt ++ code_hash)[12:]`. This is blocked on switching to Ethereum address format. |
| WAVS submission contract interface (`handleSignedEnvelope`) | WAVS aggregators require this function on submission contracts; without it WAVS cannot write state to Layer | HIGH | The `IWavsServiceHandler` interface requires a `handleSignedEnvelope()` function. Layer must host Ewasm contracts that implement this interface. The service manager validates aggregated operator signatures before accepting state. |
| AVS operator state write path | WAVS operators must be able to submit signed results that get written to persistent Layer state | HIGH | This is the core new capability. Operators sign off-chain computation results, aggregator bundles them, submits to Layer via `handleSignedEnvelope`. Chain verifies threshold signatures, writes result to contract state. |
| State root / app hash computation | Commonware uses application-defined block format; the app hash must commit to the current state tree for consensus validity | HIGH | Currently returns RocksDB state hash via Tendermint commit path. Must be replaced: compute a Merkle root (MMR or Patricia trie) over module state and return it as the consensus-level state commitment. |
| Block lifecycle hooks (BeginBlock / EndBlock) | Root contract uses BeginBlock/EndBlock for protocol lifecycle; removing them breaks chain-managed contracts | MEDIUM | Already exists via `SudoMsg::BeginBlock{}` and `SudoMsg::EndBlock{}` dispatched to the root contract. Must be preserved in Commonware block processing (called in `propose()` / after finalization). |
| JSON-RPC or HTTP API for external clients | Without an API, WAVS components, operators, and users cannot query chain state or submit transactions | MEDIUM | Currently gRPC + REST gateway. For Ethereum compatibility, consider adding an Ethereum JSON-RPC endpoint (eth_call, eth_sendRawTransaction, eth_getStorageAt). The existing gRPC layer can remain for internal use. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Bidirectional WAVS state (Layer as AVS state store) | WAVS programs today have no persistent state — they must treat each execution as stateless. Layer breaks this constraint, giving AVS programs a chain-backed state store that survives across operator rotations. | HIGH | WAVS design docs explicitly flag persistent state as unsupported in the base model. Layer becomes the external state store that WAVS workflows read from. Operators write results to Layer; subsequent WAVS components query Layer state at specific block heights (deterministic). |
| Ethereum-native contract types on non-EVM chain | Layer runs WASM (not bytecode) contracts but exposes Ethereum types (20-byte addresses, ABI encoding, keccak256 slots). Developers write Rust/AssemblyScript contracts that look like EVM contracts without EVM overhead. | HIGH | This is the Ewasm proposition: WASM performance + Ethereum type compatibility. Differentiated from CosmWasm (different types) and from pure EVM chains (different runtime). |
| zkVM state rollup to Ethereum | Layer state can be proven on Ethereum with a ZK proof, giving WAVS programs Ethereum-level finality guarantees for their persistent state without requiring all state to live on Ethereum. | VERY HIGH | SP1 (Succinct) is the most mature option: 1.48MB proof size, ~10s proving time on GPU cluster, production deployments in Polygon zkEVM v5. RISC0 is viable alternative. Both require Rust guest programs (satisfied). The rollup contract on Ethereum receives state roots + validity proofs. |
| Commonware consensus (200ms blocks, 300ms finality) | Commonware's simplex delivers ~200ms block times and ~300ms finality — dramatically faster than Tendermint (~1s blocks, ~5s finality). Faster finality means AVS state writes are available to WAVS programs sooner. | HIGH | Benchmarked via Alto: 20% block time reduction to ~200ms, 65% CPU reduction vs equivalent Tendermint setup. But Commonware is ALPHA; production readiness must be monitored. |
| EigenLayer restaking security for state guarantees | WAVS uses EigenLayer restaked ETH to economically secure operator behavior. Layer benefits from this: state written by WAVS operators is backed by slashable stake, not just chain validators. | MEDIUM | This is EigenLayer's AVS security model applied to persistent state. No additional implementation required on Layer's side; falls out of WAVS integration. Requires correct service manager deployment. |
| Block-level state queries for WAVS determinism | WAVS requires all operators to produce identical results (exact match aggregation). Layer can serve state queries at specific block heights, enabling deterministic WAVS component execution. | MEDIUM | WAVS design docs specify that "Ethereum queries at specific block heights" are an approved deterministic data source. Layer must support historical state queries (not just latest state). This requires either state snapshots or a queryable Merkle history. |
| Root contract governance over chain upgrades | An on-chain governance contract (root contract) can promote/demote system contracts, set begin/end blockers, and trigger chain-level migrations — without hard forks. | MEDIUM | Already partially implemented. Needs to be preserved through migration and extended with Commonware validator set management (the TODO comment at `contracts/root/src/msg.rs:39`). |
| Fine-grained contract permissions system | Instead of all-or-nothing root privilege, individual contracts get promoted with specific capabilities. | MEDIUM | Currently a TODO in `GovMsg::PromoteContract`. Implementing partial privileges makes the system safer and enables more composable protocol designs. |

### Anti-Features (Deliberately Not Building)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Cosmos SDK / CosmJS compatibility | Existing tooling; developers know Keplr/CosmJS | The entire point of this revitalization is to switch to Ethereum types. Maintaining Cosmos compatibility means maintaining two type systems indefinitely — double the encoding logic, double the surface area, guaranteed drift. The project explicitly lists this as out of scope. | Use Ethereum JSON-RPC for client compatibility. ethers.js / viem / alloy work out of the box. |
| Solidity-to-WASM compilation as primary target | Solidity developers are the largest pool of contract developers | Solidity-to-WASM toolchains (e.g. solang) are immature and produce suboptimal WASM. Primary contract targets are Rust and AssemblyScript — both have mature, well-tested WASM toolchains. Solidity support can come later if demand exists. | Target Rust (via Ewasm SDK) as primary. AssemblyScript as secondary. Mark Solidity as future/experimental. |
| EVM bytecode execution (full zkEVM) | Full EVM compatibility would maximize Solidity contract portability | Running EVM bytecode is categorically different from running WASM with EEI host functions. Adding a full EVM alongside the WASM runtime doubles consensus complexity and gas model complexity. The project's goal is Ewasm (WASM + Ethereum types), not EVM. | Ewasm gives Ethereum type compatibility without bytecode compatibility. |
| Tendermint P2P gossip protocol | Existing implementation | Tendermint networking is tightly coupled to the ABCI consensus model being replaced. The codebase already depends on Commonware's p2p primitive for networking. Running both creates routing conflicts. | Commonware p2p crate handles authenticated peer-to-peer communication. |
| Sharding / state partitioning | Performance at scale | Single-chain first. Sharding introduces cross-shard communication complexity that is premature for v1. Layer's value proposition is as a purpose-built state store for WAVS, not a general-purpose high-throughput chain. | Horizontal scaling can be revisited after the WAVS integration model is validated. |
| Operator-local mutable state in WAVS components | Seemingly useful for caching | WAVS explicitly prohibits this: operators may join after a service has been running, breaking state sync. Any operator-local state risks consensus failure due to inconsistency across operators. | Use Layer chain as the external state store. Components read from Layer at a specific block height (deterministic), write results back via `handleSignedEnvelope`. |
| gRPC Cosmos API parity (all unimplemented endpoints) | Full API surface area looks complete | 14+ gRPC endpoints are currently unimplemented stubs that return errors. Implementing all of them for Cosmos compatibility is wasted effort when the type system is being replaced. Time is better spent on the Ethereum JSON-RPC surface. | Implement only the subset needed: tx submission, account queries, contract state queries, block queries. Drop Cosmos-specific endpoints (auth params, module accounts, denom metadata). |
| CosmWasm contract backward compatibility | Existing contracts would just work | CosmWasm contracts use JSON encoding and Cosmos types (`Addr`, `Coin` as string). Ewasm contracts use ABI encoding and Ethereum types. The two are fundamentally incompatible at the host function level. Maintaining a CosmWasm compatibility shim doubles the VM surface and creates ambiguity about which type system is canonical. | Provide a migration path document. Existing contracts (root, caller, echo) must be rewritten in the Ewasm model. |

---

## Feature Dependencies

```
[Commonware Automaton implementation]
    └──requires──> [State root / app hash computation]
                       └──requires──> [Merkle state tree (MMR or trie)]

[Ewasm host functions — storage I/O]
    └──requires──> [Ethereum 20-byte address system]
    └──requires──> [Ethereum ABI encoding]
    └──requires──> [Persistent key-value storage with namespacing]  (already exists)

[WAVS submission contract interface]
    └──requires──> [Ewasm host functions — storage I/O]
    └──requires──> [Ewasm host functions — execution context]
    └──requires──> [Ethereum ABI encoding]
    └──requires──> [AVS operator state write path]

[AVS operator state write path]
    └──requires──> [WAVS submission contract interface]
    └──requires──> [secp256k1 signing (Ethereum-style)]

[Deterministic contract address derivation]
    └──requires──> [Ethereum 20-byte address system]

[Gas metering (instruction-level)]
    └──requires──> [Ewasm host functions — execution context]  (useGas / getGasLeft)

[zkVM state rollup to Ethereum]
    └──requires──> [State root / app hash computation]
    └──requires──> [Commonware Automaton implementation]  (need finalized blocks to prove)

[Block-level state queries for WAVS determinism]
    └──requires──> [State root / app hash computation]
    └──requires──> [Persistent key-value storage with namespacing]

[Fine-grained contract permissions]
    └──enhances──> [Root contract governance]

[Root contract governance]
    └──requires──> [Block lifecycle hooks (BeginBlock / EndBlock)]
    └──requires──> [WAVS submission contract interface]  (root is itself an Ewasm contract)

[Ethereum JSON-RPC API]
    └──enhances──> [WAVS submission contract interface]  (operators submit via JSON-RPC)
    └──conflicts──> [Cosmos gRPC API parity]  (building both is scope creep)
```

### Dependency Notes

- **Commonware Automaton requires state root computation:** Commonware's `propose()` callback must produce a block that includes the app hash. Without a Merkle commitment over state, the block is not verifiable by other participants.
- **Ewasm storage host functions require 20-byte addresses:** The EEI uses 20-byte addresses for contract identification in calls. Storage slots use 32-byte keys (EVM model). Both require Ethereum address semantics to be in place first.
- **WAVS submission requires ABI encoding:** The `handleSignedEnvelope` function receives ABI-encoded payloads from the WAVS aggregator. The Layer Ewasm runtime must be able to decode these before the handler contract can process them.
- **zkVM rollup requires finalized blocks:** The ZK prover generates proofs of state transitions. Without finalized blocks and a committed state root, there is no state to prove. This makes rollup a v1.x feature, not v1.
- **Block-level queries enhance WAVS determinism:** WAVS components use Layer state as a deterministic data source by querying at a specific block height. Historical query support is required for this; without it WAVS components see different state depending on when they run.

---

## MVP Definition

### Launch With (v1)

These features together constitute a working WAVS state integration on Layer with Commonware consensus.

- [ ] **Commonware Automaton implementation** — Chain is dead without it. Replaces ABCI. Custom block format with app hash.
- [ ] **Ethereum 20-byte address system** — Foundation for Ewasm runtime and WAVS compatibility. Fix AccountId to drop 32-byte path and bech32.
- [ ] **Ethereum ABI encoding** — Replace CosmWasm JSON encoding throughout. Required for WAVS contract compatibility.
- [ ] **Ewasm host functions (EEI): storage + execution context** — Minimum viable contract execution. storageLoad, storageStore, getCaller, getCallValue, getBlockNumber, getBlockTimestamp, getGasLeft, useGas, finish, revert.
- [ ] **Gas metering (instruction-level)** — Required for safety. Without it a single contract can halt the chain.
- [ ] **secp256k1 signing (Ethereum-style)** — Users need to sign transactions. Must match Ethereum signing format (EIP-191 or EIP-712) for wallet compatibility.
- [ ] **Deterministic contract address derivation (CREATE2-style)** — Fixes the known bug. Required for consensus correctness.
- [ ] **WAVS submission contract interface (`handleSignedEnvelope`)** — The core integration point. Layer must host a contract implementing this to receive WAVS operator results.
- [ ] **AVS operator state write path** — Without this, WAVS operators have nowhere to write persistent state; the entire value proposition is unrealized.
- [ ] **State root / app hash computation (Merkle)** — Required for Commonware block validity. Enables block-level state queries as a downstream benefit.

### Add After Validation (v1.x)

- [ ] **Ethereum JSON-RPC endpoint** — Add once core works. Trigger: when WAVS operators need a standard interface for submitting transactions and querying state.
- [ ] **Block-level historical state queries** — Add once state root is stable. Trigger: when WAVS component developers need deterministic state reads.
- [ ] **Fine-grained contract permissions** — Add once root contract is migrated to Ewasm. Trigger: when second system contract is promoted and all-or-nothing privileges become limiting.
- [ ] **EEI inter-contract calls (call, callDelegate, callStatic, create)** — Add once single-contract execution is stable. Trigger: when contract developers need composability.
- [ ] **EEI logging (log0-log4)** — Add for contract event emission. Trigger: when WAVS trigger detection needs to subscribe to Layer contract events.

### Future Consideration (v2+)

- [ ] **zkVM state rollup to Ethereum** — Deferred because: requires finalized blocks + stable state root + zkVM integration (SP1 or RISC0) + Ethereum verifier contract deployment. High complexity, depends on all v1 features being stable. Defer until WAVS integration is validated.
- [ ] **Commonware threshold_simplex (BLS threshold signatures in certificates)** — Upgrade path from simplex. Enables lite clients and succinct consensus certificates useful for the rollup. Defer until simplex is stable.
- [ ] **Solidity/AssemblyScript contract toolchain** — Secondary contract target. Defer until Rust Ewasm SDK is working and a demand signal exists.

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Commonware Automaton implementation | HIGH | HIGH | P1 |
| Ethereum 20-byte address system | HIGH | MEDIUM | P1 |
| Ethereum ABI encoding | HIGH | HIGH | P1 |
| Ewasm host functions (EEI) — storage | HIGH | HIGH | P1 |
| Ewasm host functions (EEI) — execution context | HIGH | MEDIUM | P1 |
| Gas metering (instruction-level) | HIGH | HIGH | P1 |
| secp256k1 signing (Ethereum-style) | HIGH | MEDIUM | P1 |
| Deterministic contract address derivation | HIGH | MEDIUM | P1 |
| WAVS submission contract interface | HIGH | HIGH | P1 |
| AVS operator state write path | HIGH | HIGH | P1 |
| State root / app hash (Merkle) | HIGH | HIGH | P1 |
| Block lifecycle hooks (BeginBlock/EndBlock) | MEDIUM | LOW | P1 — already exists, must preserve |
| Nonce/sequence tracking | MEDIUM | LOW | P1 — already exists, must preserve |
| Ethereum JSON-RPC endpoint | HIGH | MEDIUM | P2 |
| Block-level historical state queries | MEDIUM | MEDIUM | P2 |
| Fine-grained contract permissions | MEDIUM | MEDIUM | P2 |
| EEI inter-contract calls | MEDIUM | HIGH | P2 |
| EEI logging (log0-log4) | MEDIUM | LOW | P2 |
| Root contract governance (validator set) | LOW | MEDIUM | P2 |
| zkVM state rollup to Ethereum | HIGH | VERY HIGH | P3 |
| Commonware threshold_simplex | MEDIUM | HIGH | P3 |
| AssemblyScript contract support | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for v1 launch
- P2: Should have, add in v1.x after core works
- P3: Future consideration, v2+

---

## Existing Layer Features: Retain vs Replace

| Feature | Status | Action |
|---------|--------|--------|
| Auth module (nonce/sequence, account store) | Keep structure, replace types | Nonce tracking and account existence checks are still needed. Replace Cosmos key format with secp256k1 → Ethereum address derivation. Drop Ed25519 stub. |
| Bank module (native token transfers) | Keep — adapt to Ethereum types | Token transfers using Ethereum addresses and ABI-encoded amounts instead of Cosmos Coin. Burn is less commonly needed but low cost to keep. |
| Wasm module (CosmWasm VM execution) | Replace VM, keep structure | The keeper pattern, contract store, code store, and CONTRACTS_BY_CODE indexing are good. Replace the CosmWasm VM backend with an Ewasm-compatible WASM executor. Replace JSON host functions with ABI host functions. |
| RocksDB storage + PrefixedStorage | Keep | Solid abstraction. Key-value semantics map cleanly to Ethereum storage slots. Prefix namespacing still needed per contract. |
| Root contract governance | Keep — rewrite in Ewasm | The permission model (promote/demote, begin/end blockers) is valuable. The contract itself must be rewritten for Ewasm types. The TODO for validator set management must be resolved. |
| gRPC Cosmos API (14+ endpoints) | Partial — retire most | Keep: tx submission, account queries, contract state queries, block info. Retire: Cosmos-specific auth params, denom metadata, module accounts, CosmWasm code/params endpoints. Replace with Ethereum JSON-RPC. |
| REST gateway (Go) | Keep or replace | Useful for JSON access. Can be repurposed to proxy Ethereum JSON-RPC instead of Cosmos gRPC. Alternatively, implement JSON-RPC directly in the Rust daemon. |
| Tracing / OpenTelemetry | Keep | Operational necessity. No changes needed. |
| Docker local node setup | Keep | Development tooling. Update to reflect new daemon structure. |

---

## Competitor / Comparable Feature Analysis

| Feature | CosmWasm chains (Cosmos) | Pure EVM chains (Ethereum L2s) | Layer + WAVS approach |
|---------|--------------------------|-------------------------------|----------------------|
| Contract language | Rust (JSON types) | Solidity (ABI types) | Rust/AssemblyScript (ABI types) — best of both |
| Address format | bech32 / 20- or 32-byte | 0x hex / 20-byte only | 0x hex / 20-byte — Ethereum compatible |
| Persistent AVS state | Not designed for it | Via smart contracts | Native integration — WAVS writes directly to chain state |
| Consensus | Tendermint / CometBFT | Various (OP stack, ZK provers) | Commonware simplex — lower latency, simpler |
| zkVM rollup | No standard path | Native (it IS the rollup) | Optional via SP1/RISC0 — post-v1 |
| WAVS integration | Not supported | Possible via EVM contracts | First-class — purpose-built |
| Gas model | Cosmos SDK gas | EVM gas (opcode-level) | Ewasm gas (instruction-level) — similar to EVM |

---

## Sources

- WAVS design considerations: https://docs.wavs.xyz/design — explicitly states persistent operator-local state is unsupported; describes deterministic execution requirements (MEDIUM confidence — current official docs)
- WAVS how it works: https://docs.wavs.xyz/how-it-works — describes `handleSignedEnvelope` interface and IWavsServiceHandler (MEDIUM confidence — current official docs)
- Ewasm EEI specification: https://ewasm.readthedocs.io/en/mkdocs/eth_interface/ — complete list of EEI host functions (HIGH confidence — official ewasm spec, though ewasm project is orphaned the spec is stable)
- ewasm_api Rust crate: https://docs.rs/ewasm_api/latest/ewasm_api/ — Rust bindings for all EEI functions (HIGH confidence — crate docs)
- Commonware consensus docs: https://docs.rs/commonware-consensus/latest/commonware_consensus/simplex/index.html — Automaton trait, CertifiableAutomaton, block interface (MEDIUM confidence — ALPHA software, API may shift)
- Commonware MMR blog: https://commonware.xyz/blogs/mmr — MMR for state commitments, lite client proofs (MEDIUM confidence)
- SP1 zkVM: https://blog.succinct.xyz/introducing-sp1/ — production-ready, 1.48MB proofs, ~10s proving time, Rust guest programs (HIGH confidence — production deployments in Polygon zkEVM v5)
- RISC0 Zeth: https://github.com/risc0/zeth — Type 0 zkEVM using RISC0, Ethereum block proving (MEDIUM confidence)
- Commonware anti-framework blog: https://commonware.xyz/blogs/commonware-the-anti-framework — explains no prescribed block format, no hardcoded execution rules (HIGH confidence)
- Existing codebase analysis: `packages/std/src/`, `packages/app/src/`, `contracts/root/src/` — direct code reading (HIGH confidence)

---
*Feature research for: WAVS-integrated Ethereum-compatible blockchain (Layer SDK)*
*Researched: 2026-03-18*
