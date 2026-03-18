# Pitfalls Research

**Domain:** Rust blockchain revitalization — consensus replacement, WASM runtime migration, zkVM integration
**Researched:** 2026-03-18
**Confidence:** MEDIUM-HIGH (core blockchain pitfalls HIGH; Commonware-specific pitfalls MEDIUM due to limited public documentation; WAVS-specific pitfalls MEDIUM)

---

## Critical Pitfalls

### Pitfall 1: Commonware Certification Non-Determinism Halts the Chain

**What goes wrong:**
The `CertifiableAutomaton::certify()` method must return the same decision on every honest node. If any code path in block validation can produce different results across nodes — even due to HashMap iteration order, wall-clock time, or float arithmetic — the chain will stall. Commonware's docs are explicit: "The decision returned by certify must be deterministic and consistent across all honest participants to ensure liveness." A single node diverging causes the view to never reach quorum.

**Why it happens:**
Developers port existing Tendermint `DeliverTx` logic into `certify()` without auditing for determinism. Rust's `HashMap`/`HashSet` use random seeds by default, meaning iteration order differs per process. `SystemTime::now()` returns different values. Any external I/O (reading config files, env vars) also differs.

**How to avoid:**
- Replace all `HashMap`/`HashSet` in consensus-critical paths with `BTreeMap`/`BTreeSet` (deterministic iteration order).
- Prohibit `SystemTime::now()` — use block header timestamps only.
- Prohibit floating-point arithmetic in `certify()` logic. Use integer fixed-point math.
- Run determinism fuzz tests: execute the same block twice in the same process with different random seeds and assert identical output.
- Establish a "determinism boundary" in code: annotate which modules are consensus-critical and enforce via code review.

**Warning signs:**
- Nodes produce different `AppHash` after processing the same block.
- The chain stalls at a specific view height with some nodes voting differently.
- Test suite passes locally but fails in CI with different OS/hardware.
- Any `HashMap` iteration in `certify()`, block proposal, or state transition code.

**Phase to address:**
Consensus replacement phase (Tendermint → Commonware). Before any block can be finalized, the determinism contract must be established and tested.

---

### Pitfall 2: Unsafe WASM VM Lifetime Violation Causes Silent Memory Corruption

**What goes wrong:**
The existing `danger_will_robinson` function in `packages/app/src/wasm/vm/backend.rs` uses `unsafe transmute` to coerce lifetimes of borrowed pointers into `'static` references. This allows the Wasmer instance cache to hold references that may outlive the underlying data. If the Ewasm migration adds new code paths that drop the underlying store before the static reference is used — for instance, in async contexts, worker threads, or a new Commonware async block executor — the result is use-after-free and potential memory corruption. The bug is silent: Rust's borrow checker is bypassed by the unsafe transmute.

**Why it happens:**
The existing unsafe code has a narrow, manually-enforced safety contract. When integrating a new consensus layer (which likely has a different execution model and task scheduler), developers add new call sites without understanding the lifetime invariants. The unsafe block's "scary name" is a warning, not a machine-enforced guarantee.

**How to avoid:**
- Before touching the WASM VM code, formally document the safety invariants of `danger_will_robinson` as `# SAFETY` comments listing exactly which lifetimes must outlive which.
- Run the test suite under `cargo miri` to catch any existing lifetime violations before adding new code.
- When integrating Commonware, treat the WASM executor as a self-contained unit with a well-defined lifetime scope. Do not share WASM instance cache handles across async task boundaries.
- As part of the Ewasm migration, refactor away from transmute: use self-referential structs with `ouroboros` or `rental`, or restructure the cache to be owned rather than borrowed.

**Warning signs:**
- Any new `spawn` or `tokio::task::spawn_blocking` calls that capture a WASM backend reference.
- Segfaults or memory errors that appear only under load or with concurrent contract execution.
- MIRI errors during `cargo miri test`.
- Any function that returns a reference into the WASM instance cache across an await point.

**Phase to address:**
Must be addressed before the Ewasm migration phase begins. The unsafe code may be structurally incompatible with the new runtime; migrating without fixing it first embeds a time bomb into the new codebase.

---

### Pitfall 3: Address Type Migration Silently Corrupts Existing State

**What goes wrong:**
The entire existing state — accounts, balances, contract addresses, contract-stored keys — is keyed using Cosmos bech32 addresses (variable length, ~45 chars). Ethereum addresses are 20-byte H160 values. When the type migration replaces `AccountId`/bech32 with H160, all storage keys change. Existing on-chain data becomes inaccessible because the old keys no longer match. The problem is silent: the new code compiles and runs, but `auth::keeper::get_account()` returns `None` for every address, balances appear as zero, and contracts appear uninstantiated — because the key prefix no longer matches.

**Why it happens:**
Developers replace the type alias and recompile. The storage layer (`storage::plus::map`) encodes the type as a key using `KeyDeserialize`. Changing the type changes the encoded key bytes. Old data is still present in RocksDB under the old byte layout, but the new key encoder produces different bytes and misses it.

**How to avoid:**
- Write an explicit state migration that re-encodes all keys from bech32 to H160 format. This is non-negotiable.
- For each keeper (`auth`, `bank`, `wasm`), create a migration function that iterates the old prefix, decodes with the old deserializer, re-encodes with the new deserializer, writes the new key, and deletes the old key — all within a single atomic RocksDB WriteBatch.
- Add an integration test that: (1) seeds state with bech32 accounts, (2) runs the migration, (3) asserts all accounts are readable under H160 keys, (4) asserts no data remains under bech32 keys.
- Block the address type change behind a feature flag until the migration is complete and tested.

**Warning signs:**
- Any code change that touches `AccountId` type aliases or imports.
- RocksDB contains data but queries return empty results after the type change.
- Tests that pass with in-memory `MemoryStore` but fail with RocksDB (because in-memory tests don't have pre-existing bech32 keyed data).

**Phase to address:**
Ewasm/type migration phase. This is the single highest-risk data integrity issue in the entire migration.

---

### Pitfall 4: Non-Deterministic Contract Address Generation Breaks Consensus Replay

**What goes wrong:**
`packages/app/src/wasm/keeper.rs:1001` is explicitly marked as non-deterministic. If contract instantiation produces different addresses on different nodes, each node will have a different contract registry, all believing their block is correct. The consensus engine will fail to reach agreement or, worse, will agree on a block but diverge on state: node A's state has contract at address X, node B's state has it at address Y.

**Why it happens:**
The current implementation likely uses something time-dependent or process-local (e.g., random bytes, counter not initialized from consensus state) for address derivation. This pre-existed the 2-year dormancy and was deferred.

**How to avoid:**
- Before any Commonware integration, fix address generation to be fully deterministic from block-provided inputs: `hash(deployer_address || tx_hash || instantiation_nonce)`.
- The nonce must come from consensus state (the account nonce or block-level counter), not from system time or local state.
- Add a test that instantiates the same contract with the same parameters twice from a replay of the same block and asserts identical addresses result.

**Warning signs:**
- The `// TODO: non-deterministic` or `// FIXME` comment at `wasm/keeper.rs:1001`.
- Different nodes in a testnet showing different contract registry contents.
- Replay of a historical block produces different state hash.

**Phase to address:**
Must be fixed in the earliest Commonware integration phase, before any testnet is spun up. This is a consensus-correctness blocker.

---

### Pitfall 5: Commonware Write-Ahead Log Not Synced Before Broadcast

**What goes wrong:**
Commonware's Voter requires that its write-ahead log (WAL) be synced to disk before messages are broadcast. If a node crashes between broadcasting a vote and persisting it, the node may re-broadcast a conflicting vote upon restart — which is Byzantine behavior even if unintentional. Commonware's docs explicitly state: "the Voter syncs its write-ahead log before message broadcast to prevent Byzantine behavior after unclean shutdowns." If the application wraps the Voter incorrectly (e.g., batching disk writes for performance), it risks this behavior.

**Why it happens:**
Developers new to Commonware optimize for performance by batching or async-flushing the WAL, not realizing the safety model depends on synchronous WAL writes before broadcast.

**How to avoid:**
- Do not configure async WAL flushing for the Voter component.
- Use `commonware_p2p::authenticated` for all peer connections — unauthenticated P2P disables lazy batch verification safety guarantees.
- Do not implement custom WAL logic; use Commonware's built-in journal primitives exactly as documented.
- Add crash-recovery tests that kill the node during voting and assert it resumes correctly without producing conflicting votes.

**Warning signs:**
- Custom async flush configurations on the Voter's underlying storage.
- Any code that defers or batches WAL writes for the Voter.
- Using `commonware_p2p::unauthed` or equivalent unauthenticated transport.

**Phase to address:**
Commonware integration phase. Configuration must be validated before any multi-node testing.

---

### Pitfall 6: zkVM Guest/Host Boundary Moves Security Outside the Proof

**What goes wrong:**
In SP1/RISC Zero, only the guest program is cryptographically proven. All host code — including the code that feeds inputs to the guest — receives zero security guarantees. If Layer state transition logic lives in the host and only a summary (e.g., a final state root) is proven in the guest, the proof is worthless: a malicious prover can feed a false state root as input, and the proof will be valid but the state will be fraudulent. This is the single most common zkVM integration mistake.

**Why it happens:**
Developers design the proof around proving "I computed X" rather than "X is a valid transition from state S under the Layer state machine rules." The state machine logic is complex, so it gets left in the host. The guest becomes trivially small — which should be a warning sign.

**How to avoid:**
- The state transition function (all keeper logic, gas metering, account updates) must execute inside the zkVM guest.
- The guest must receive the full pre-state (or a Merkle proof of relevant state), execute all transactions, and produce the post-state root as a public output.
- The host is limited to: fetching pre-state data, submitting input to the guest, and relaying the proof to Ethereum.
- Design rule: if the guest can be replaced with `output = input.claimed_result` without changing proof validity, the security model is broken.

**Warning signs:**
- Guest program is under ~500 lines of logic.
- Host code validates state transition results before passing them to the guest.
- The guest's public outputs are just a hash the host computed outside the guest.
- No Layer keeper/module code appears in the guest crate's dependency tree.

**Phase to address:**
zkVM rollup phase. The proof boundary must be defined in design before implementation begins — retrofitting it is extremely expensive.

---

### Pitfall 7: Cargo Dependency Version Conflicts After 2-Year Dormancy

**What goes wrong:**
The codebase pins CometBFT v0.38.12, CosmWasm 1.5.4, and Tendermint 0.39.1. After 2 years, transitive dependencies will have had major version bumps. When adding Commonware (which has its own tokio, bytes, prost, and crypto crate dependencies), Cargo will attempt to resolve a unified dependency graph. Type incompatibilities between old pinned crates and Commonware's crate versions will cause compilation failures that look like type errors, not dependency conflicts.

Specific risks:
- `prost` (protobuf): major version between 0.11 and 0.13; proto-generated types are not compatible across versions.
- `tokio`: currently 1.x, but minor version requirements for specific APIs differ.
- `bytes`: `Bytes` type is not compatible between 0.x and 1.x; any crate mixing versions will fail at API boundaries.
- `ring`/`rustls`/`openssl`: crypto crates have had multiple breaking changes.

**Why it happens:**
Each new crate added (Commonware, zkVM SDK, WAVS client) brings its own pinned transitive dependencies. Cargo's resolver may select incompatible versions for types that must be the same across the API boundary.

**How to avoid:**
- Run `cargo tree -d` immediately after adding each new dependency to identify duplicate versions.
- Use `cargo update` on a fresh branch before any new dependencies are added to upgrade all existing transitive deps to their latest compatible version first.
- Patch conflicts using `[patch.crates-io]` in the workspace `Cargo.toml` to force a single version where safe.
- Address `prost` version conflicts first — generated proto code must use the exact same prost version as the consumer.
- Pin Commonware, WAVS, and zkVM crates to versions that share the same `tokio`, `bytes`, and `prost` major versions as the rest of the workspace.

**Warning signs:**
- Multiple entries for the same crate in `cargo tree` output.
- Compiler errors about type mismatches on `Bytes`, `Message`, or tokio `Runtime` across module boundaries.
- `error[E0308]: mismatched types` where both sides look like the same type but are from different crate versions.

**Phase to address:**
The very first task in any phase that adds new dependencies. Dependency audit should precede all other work.

---

### Pitfall 8: CosmWasm Gas Mispricing Vulnerability Still Present in Fork

**What goes wrong:**
CWA-2024-004 documents that cosmwasm-vm versions before 1.5.7 (and 2.0.0–2.0.5, 2.1.0–2.1.2) have gas mispricing that allows certain WASM opcodes to consume approximately 10x more computation than their gas cost suggests. The fork will be based on CosmWasm 1.5.4, which is within the affected range. A malicious contract can exploit this to halt the chain via denial-of-service.

**Why it happens:**
The fork branches from the last known-good version (1.5.4) without forward-porting security patches. Security advisories from upstream are not tracked.

**How to avoid:**
- Immediately cherry-pick the gas pricing fix from CosmWasm 1.5.7 into the fork.
- Set up a process to monitor `github.com/CosmWasm/advisories` for new advisories that affect the fork's code lineage.
- Add a CI check that compares the fork's gas metering constants against the patched upstream version.

**Warning signs:**
- Contracts that consume unexpectedly large amounts of wall-clock time despite fitting within gas limits.
- Any benchmark showing WASM execution time disproportionate to gas units consumed.

**Phase to address:**
CosmWasm fork phase, day one. Before the fork is used for any purpose, the security patch must be applied.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Keep `danger_will_robinson` transmute during Ewasm migration | Avoids refactoring unsafe code upfront | Embeds a memory corruption risk that is incompatible with async Commonware executor | Never — fix before migrating |
| Leave bech32 addresses in storage, add Ethereum addresses as aliases | Faster initial migration | Dual-format state doubles storage complexity and creates inconsistency bugs | Never — one canonical format |
| Hardcode gas limits from `app.rs` without fixing consensus-break risk | Less initial work | Different nodes can run with different limits, causing non-determinism if limits gate execution | Only acceptable until testnet; must be fixed before mainnet |
| Use `HashMap` in any module that touches `certify()` | Familiar data structure | Non-deterministic iteration order halts consensus | Never in consensus-critical paths |
| Keep `.unwrap()` in `parse_keys` during migration | Faster to port than to fix | Corrupted or migrated storage keys panic the node | Never — fix during type migration when keys are already being changed |
| Git submodule pointing at upstream CosmWasm instead of fork | Easier to pull upstream | Upstream changes break the custom host functions without warning | Never — fork must be the canonical source |
| Prove only the state root in the zkVM guest, not the full transition | Smaller guest program | Proof is cryptographically valid but provides no security | Never — defeats the purpose of zkVM |

---

## Integration Gotchas

Common mistakes when connecting to external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Commonware Simplex | Implementing `certify()` with any side effects or I/O | `certify()` must be pure and deterministic; all I/O happens in the Application's async context outside `certify()` |
| Commonware P2P | Using unauthenticated transport (`commonware_p2p::unauthed` or equivalent) | Always use `commonware_p2p::authenticated`; lazy batch verification is unsafe without it |
| Commonware Batcher | Blocking the Batcher waiting for Application verification | Commonware's design is non-blocking; the Voter proceeds independently. Blocking breaks the performance model. |
| WAVS state writes | Accepting AVS operator writes without replay protection | WAVS messages must include a nonce or sequence number checked against on-chain state to prevent replay attacks |
| WAVS state reads | Returning live RocksDB state in a WAVS response without snapshot isolation | A concurrent block execution can mutate state between the WAVS read request and response, returning inconsistent data |
| zkVM guest | Using `std::time`, `rand::random()`, or environment variables in guest code | The zkVM guest runs in a sandboxed RISC-V environment without OS support; use only zkVM-provided entropy and clock abstractions |
| zkVM host | Trusting host-computed values that the guest uses without re-deriving or verifying them | The host is untrusted; the guest must re-derive or validate all security-critical inputs from committed pre-state |
| CosmWasm fork submodule | Running `cargo update` in the submodule root instead of the workspace root | Submodule dependency changes are not visible to the parent workspace unless the workspace `Cargo.toml` references the submodule's local path |
| wreth Ethereum rollup | Submitting state roots to Ethereum without a commitment scheme for the pre-state | The on-chain verifier needs to know which pre-state root the proof started from; include it as a public input |

---

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| `.unwrap()` panics in `parse_keys` iterators | Node crashes during full state iteration; hits corrupted key in RocksDB after migration | Replace all unwraps in `parse_keys` with proper error handling before state migration | Immediately after any state format change |
| Unbound WASM linear memory per contract instance | Memory usage grows unbounded under concurrent contract execution | Set explicit linear memory limits per Wasmer instance in the Ewasm VM configuration | At ~50+ concurrent contract instances |
| 288 `clone()` calls in keeper hot paths | Transaction throughput saturates CPU/memory at modest TPS | Profile with `cargo flamegraph` and replace clone-heavy paths with `Arc` or references | At ~100 TPS depending on contract complexity |
| Page size hardcoded at 100 in WASM keeper queries | gRPC query responses truncate silently; clients receive partial data without knowing | Make page size configurable; add `next_page_token` to all list responses | When any collection exceeds 100 items |
| Synchronous RocksDB reads inside block execution | Block execution time grows linearly with state size; consensus timeout exceeded | Batch reads using RocksDB's MultiGet; prefetch predictable state during block proposal | At ~1M state entries |
| zkVM proof generation with unbounded transaction count per block | Proof generation time exceeds block time target | Cap transactions per provable block; implement chunked proving with aggregation | At ~50+ complex transactions per block (rough estimate; benchmark required) |

---

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Non-deterministic address generation (existing bug) | Chain halt or state divergence across nodes | Fix derivation to `hash(deployer || tx_hash || nonce)` before testnet |
| CosmWasm gas mispricing (CWA-2024-004) | Denial-of-service via maliciously crafted contracts | Cherry-pick upstream gas pricing fix from CosmWasm 1.5.7 into the fork |
| Malformed storage keys cause panic (existing bug) | Any node with corrupted state crashes; chain is DoS-able if an attacker can write malformed keys | Replace all `parse_keys` unwraps with error-returning variants |
| WASM VM unsafe transmute | Use-after-free in contract execution under concurrent load | Audit with Miri; refactor before adding new async execution contexts |
| No replay protection on WAVS state writes | AVS operators can replay old write transactions, reverting state | Include monotonic sequence numbers in WAVS write messages; validate on-chain |
| zkVM guest trusts host-provided inputs | Malicious prover generates valid proofs for invalid state transitions | All security-critical state must be derived or validated inside the guest, not passed in from the host |
| Hardcoded gas limits allow consensus divergence | Nodes compiled with different constants disagree on whether a transaction succeeds | Move gas configuration to consensus state (genesis or governance); validate equality at startup |
| Commonware using unauthenticated P2P | Lazy batch verification is skipped; Byzantine peers can inject invalid messages | Enforce `commonware_p2p::authenticated` in configuration |
| Git submodule fork diverges silently from security patches | Known vulnerabilities in upstream CosmWasm go unpatched in the fork | Subscribe to `CosmWasm/advisories`; add CI to diff security-relevant files against patched upstream |

---

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Consensus migration:** Commonware node starts and produces blocks — but verify no `HashMap` iteration in `certify()`, WAL sync is enabled, and multi-node testnet reaches consensus (single-node "consensus" trivially works).
- [ ] **Type migration (Cosmos → Ethereum):** New code compiles and unit tests pass — but verify all existing RocksDB state is re-encoded under new key format via integration test with pre-seeded data.
- [ ] **Ewasm runtime:** Contracts compile and execute — but verify gas metering uses the patched constants from CosmWasm 1.5.7, and WASM linear memory limits are enforced per-instance.
- [ ] **WAVS integration:** AVS operators can write state — but verify writes include replay protection, and verify that WAVS reads use snapshot isolation and do not return mid-block state.
- [ ] **zkVM rollup:** Proofs are generated and verified on Ethereum — but verify the full state transition function (all keeper logic) runs inside the guest, not just a pre-computed hash.
- [ ] **Address generation:** Contract instantiation works — but verify the same inputs produce the same address on two independent nodes processing the same block (replay test).
- [ ] **CosmWasm fork submodule:** Fork compiles — but verify the upstream security patches (CWA-2024-004) are cherry-picked and CI can detect future upstream advisories.
- [ ] **Dependency upgrade:** Codebase compiles with new dependencies — but run `cargo tree -d` to confirm no duplicate major versions of `prost`, `bytes`, or `tokio`.

---

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Chain halt from non-determinism in `certify()` | HIGH | Identify diverging node via log comparison; hot-fix the non-deterministic operation; coordinate restart from last agreed height |
| Address type migration corrupts state | HIGH | Restore from pre-migration snapshot; rewrite migration function; re-run on snapshot |
| Use-after-free in WASM VM (silent corruption) | HIGH | Memory corruption is not recoverable from; state may be inconsistent across nodes; requires restart from known-good snapshot plus code fix |
| Dependency conflict blocks compilation | MEDIUM | `cargo tree -d` to identify conflict; `[patch.crates-io]` to force single version; test thoroughly |
| Gas mispricing exploited (DoS via CWA-2024-004) | MEDIUM | Emergency upgrade to patched gas constants; halt in-flight transactions; coordinated chain restart |
| Non-deterministic contract addresses discovered on testnet | MEDIUM | Wipe testnet state; fix address generation; redeploy |
| zkVM guest does not include full state transition | HIGH (rewrite) | Restructure guest to include all keeper logic; proof boundary changes require complete re-architecture of the rollup layer |
| WAVS write replay attack | MEDIUM | Deploy on-chain nonce validation contract; replay already-written entries to restore correct state |

---

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Consensus non-determinism (HashMap, floats, time) | Commonware integration phase | Multi-node testnet processes 100 blocks; all nodes produce identical AppHash; replay test from genesis |
| Unsafe WASM VM lifetime violation | Before Ewasm migration begins | `cargo miri test` passes on all wasm/vm tests; no unsafe transmute in code touching new async contexts |
| Address type migration corrupts state | Ewasm/type migration phase (first task) | Integration test: seed bech32 state, run migration, assert all accounts readable under H160 keys, no bech32 keys remain |
| Non-deterministic contract address generation | Earliest Commonware integration phase | Replay test: same block twice → identical contract addresses |
| Commonware WAL sync configuration | Commonware integration phase | Crash-recovery test: kill node mid-vote; restart; assert no conflicting votes produced |
| zkVM guest/host boundary | zkVM design phase (before implementation) | Audit: all keeper modules appear in guest dependency tree; guest program size reflects full state transition logic |
| Cargo dependency conflicts | First task of any phase adding dependencies | `cargo tree -d` shows no duplicate major versions of `prost`, `bytes`, `tokio` |
| CosmWasm gas mispricing (CWA-2024-004) | CosmWasm fork phase, day one | Diff fork's gas constants against CosmWasm 1.5.7 patched version |
| Malformed storage keys panic | Ewasm/type migration phase (alongside key re-encoding) | Fuzz test: send malformed keys to all `parse_keys` functions; assert graceful error, not panic |
| WAVS replay protection | WAVS integration phase | Replay attack test: submit same WAVS write twice; assert second write rejected with sequence error |
| Hardcoded gas limits | Pre-testnet configuration phase | Multi-node test: compile two nodes with different gas constants; verify startup validation catches divergence |

---

## Sources

- Commonware Simplex docs (HIGH confidence): [commonware_consensus::simplex](https://docs.rs/commonware-consensus/latest/commonware_consensus/simplex/index.html)
- Zellic Cosmos security primer (HIGH confidence): [Exploring Cosmos: A Security Primer](https://www.zellic.io/blog/exploring-cosmos-a-security-primer/)
- SP1 zkVM security guide (HIGH confidence): [SP1 and zkVMs: A Security Auditor's Guide](https://blog.sigmaprime.io/sp1-zkvm-security-guide.html)
- CosmWasm security advisory CWA-2024-004 (HIGH confidence): [CosmWasm Advisories](https://github.com/CosmWasm/advisories/blob/main/CWAs/CWA-2024-004.md)
- CosmWasm 2.0 migration notes (MEDIUM confidence): [CosmWasm 2.0 Blog Post](https://medium.com/cosmwasm/cosmwasm-2-0-bbb94126ce6f)
- CosmWasm MIGRATING.md (HIGH confidence): [github.com/CosmWasm/cosmwasm/blob/main/MIGRATING.md](https://github.com/CosmWasm/cosmwasm/blob/main/MIGRATING.md)
- Cosmos EVM address derivation pitfalls (MEDIUM confidence): [Adding EVM to an Existing Chain](https://docs.cosmos.network/evm/next/documentation/migrations/add-evm-to-existing-chain)
- Rust HashMap non-determinism (HIGH confidence): [Rusty Garbage — My HashMap is non-deterministic](https://medium.com/@draft1967/rusty-garbage-my-hashmap-is-non-deterministic-0e518be0c5c6)
- rust-rocksdb CHANGELOG (HIGH confidence): [rust-rocksdb/CHANGELOG.md](https://github.com/rust-rocksdb/rust-rocksdb/blob/master/CHANGELOG.md)
- RISC Zero guest code requirements (HIGH confidence): [Guest Code 101 — RISC Zero](https://dev.risczero.com/api/zkvm/guest-code-101)
- Existing codebase concerns: `.planning/codebase/CONCERNS.md` (HIGH confidence — first-party audit)

---

*Pitfalls research for: Layer SDK revitalization — consensus migration, WASM runtime replacement, zkVM rollup integration*
*Researched: 2026-03-18*
