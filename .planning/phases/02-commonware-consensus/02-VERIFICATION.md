---
phase: 02-commonware-consensus
verified: 2026-03-19T18:10:00Z
status: gaps_found
score: 12/14 must-haves verified
re_verification: false
gaps:
  - truth: "Each finalized block carries a BLS12-381 threshold signature certificate stored in the Block.certificate field"
    status: failed
    reason: "Block.certificate field exists in the struct but execute_block() always passes certificate: None to finalize_block(). The Reporter (LayerReporter) receives the certificate bytes in Finalization activity and LOGS them, but does not write them back into the committed Block. No code path in the repository sets certificate: Some(cert_bytes) anywhere at runtime."
    artifacts:
      - path: "app/slay3rd/src/node.rs"
        issue: "execute_block() constructs Block with certificate: None (line 166) and calls finalize_block. The comment says 'LayerReporter updates the block record' but this never happens — the Reporter only logs."
      - path: "app/slay3rd/src/main.rs"
        issue: "LayerReporter::report() receives Finalization activity with cert_bytes (line 117) and logs it (line 123) but does not store it in the App or associate it with a block by height."
    missing:
      - "In execute_block(), after calling app.finalize_block(block), the certificate needs to be inserted into the committed block record. Either: (a) restructure certify() to accept the certificate from a shared channel and pass it to finalize_block before calling it, or (b) add a post-certify update path in the App to set Block.certificate by height after the Reporter delivers the cert."
      - "Alternatively, if the Commonware certify() callback is guaranteed to execute BEFORE the Reporter fires the Finalization activity, restructure so certify() awaits the certificate via oneshot from the Reporter before returning."
  - truth: "3-node local testnet reaches consensus and all nodes produce identical AppHash for the same block height"
    status: partial
    reason: "The 3-node testnet cannot function because Phase 2 uses in-process simulated P2P (commonware_p2p::simulated). Each slay3rd OS process creates its own isolated Network instance with no cross-process message passing. The three processes cannot exchange votes, certificates, or resolver messages. This is a documented architectural limitation deferred to Phase 3 (real P2P). The single-node consensus logic is correct, but multi-node consensus is physically impossible with the Phase 2 implementation."
    artifacts:
      - path: "app/slay3rd/src/main.rs"
        issue: "Uses commonware_p2p::simulated::Network (line 376) which has no cross-process transport. Each node process is isolated."
      - path: "scripts/testnet.sh"
        issue: "Launches 3 separate OS processes. They will NOT exchange consensus messages — each runs a single-node simulated network."
    missing:
      - "Phase 3: Replace commonware_p2p::simulated with commonware_p2p::authenticated to enable cross-process P2P. This is explicitly deferred and documented in STATE.md."
      - "Note: This gap is intentionally deferred to Phase 3. The consensus logic (LayerNode, CertifiableAutomaton) is complete. Only the transport layer is missing."
human_verification:
  - test: "Run cargo test -p slay3rd and confirm all 18+ unit tests pass (node, block, mempool, relay, config)"
    expected: "cargo test -p slay3rd exits 0 with all tests passing including LayerNode genesis/propose/verify/certify tests"
    why_human: "Build environment needed to confirm the async wasmer-SIGBUS mitigation (sync tests with block_on) still works correctly on the target machine"
  - test: "Run cargo build --workspace and confirm workspace compiles cleanly with all Commonware 2026.3.0 deps"
    expected: "cargo build --workspace exits 0"
    why_human: "Dependency resolution may differ across machines; the test confirms the declared workspace deps resolve correctly"
  - test: "Run cargo build --manifest-path tools/generate-testnet-keys/Cargo.toml and confirm keygen tool compiles"
    expected: "exits 0; runs tools/generate-testnet-keys to produce validator-{0,1,2}/keys.json"
    why_human: "Standalone tool outside workspace needs its own build verification"
  - test: "Run cargo build --manifest-path tools/verify-cert/Cargo.toml and confirm verifier compiles"
    expected: "exits 0"
    why_human: "Standalone tool outside workspace"
---

# Phase 2: Commonware Consensus Verification Report

**Phase Goal:** Integrate Commonware consensus engine into the Layer SDK, replacing the previous consensus stub with a working BLS threshold consensus system using the Commonware simplex protocol.
**Verified:** 2026-03-19T18:10:00Z
**Status:** gaps_found
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | slay3rd crate exists in workspace with all 5 commonware-* dependencies | VERIFIED | `app/slay3rd/Cargo.toml` contains all 5: commonware-consensus, commonware-p2p, commonware-cryptography, commonware-runtime, commonware-storage plus 4 more Commonware crates |
| 2 | No HashMap or HashSet in consensus-critical code paths (bank/keeper.rs, wasm/vm/backend.rs, wasm/vm/cache.rs) | VERIFIED | `bank/keeper.rs` uses BTreeMap, `backend.rs` uses BTreeMap, `cache.rs` retains HashSet with DETERMINISM-SAFE comment at cosmwasm_vm boundary; zero HashMap/HashSet in `app/slay3rd/src/` |
| 3 | workspace compiles with Commonware crates | VERIFIED (inferred) | All 8 phase commits exist; SUMMARY.md Self-Check shows `cargo build --workspace` clean; code is syntactically well-formed |
| 4 | LayerNode wraps Arc<Mutex<App<T>>> and implements CertifiableAutomaton | VERIFIED | `node.rs` lines 44-60: struct field `app: Arc<Mutex<App<T>>>`, line 205: `impl Automaton for LayerNode`, line 307: `impl CertifiableAutomaton for LayerNode` |
| 5 | genesis() returns initial AppHash as 32-byte digest | VERIFIED | `node.rs` lines 213-228: genesis() locks app, calls app.app_hash(), normalises to [u8; 32] via sha256::Digest |
| 6 | propose() drains mempool and returns BlockPayload digest | VERIFIED | `node.rs` lines 231-278: propose() drains mempool, builds BlockPayload, computes digest, stores in pending_payloads, sends via oneshot |
| 7 | verify() validates without mutating App state | VERIFIED | `node.rs` lines 281-304: verify() only reads pending_payloads (no app lock, no finalize_block call) |
| 8 | certify() calls App::finalize_block() | VERIFIED | `node.rs` lines 312-328: certify() calls execute_block() which calls app.finalize_block(block) at line 171 |
| 9 | BlockPayload is bincode-serialized for deterministic encoding | VERIFIED | `block.rs` lines 30-46: to_bytes() uses bincode::serialize, digest() computes SHA-256 of serialized bytes |
| 10 | Mempool accepts transactions via submit() and drains via drain_batch() | VERIFIED | `mempool.rs` lines 27-42: submit() and drain_batch() implemented with FIFO VecDeque |
| 11 | pending_payloads() accessor returns shared Arc for Relay wiring | VERIFIED | `node.rs` lines 98-106: public fn pending_payloads() returns Arc clone; `main.rs` line 276: relay created with layer_node.pending_payloads() |
| 12 | NodeConfig, Relay (LayerRelay), and main.rs consensus entry point exist and are wired | VERIFIED | config.rs (135 lines), relay.rs (260 lines), main.rs (484 lines) all exist; main.rs wires LayerNode -> LayerRelay -> SimplexConfig -> Engine |
| 13 | Each finalized block carries BLS certificate stored in Block.certificate | FAILED | Block.certificate field EXISTS in Block struct but execute_block() always passes certificate: None to finalize_block(). Reporter logs cert_bytes but never writes them to the stored block. No code path sets certificate: Some(...) at runtime. |
| 14 | 3-node testnet reaches consensus with identical AppHash | PARTIAL | testnet.sh and verify-consensus.sh exist and are syntactically valid. Multi-node consensus is architecturally impossible in Phase 2 because commonware_p2p::simulated cannot span OS processes — each node is isolated. Documented as Phase 3 prerequisite. |

**Score:** 12/14 truths verified

---

### Required Artifacts

| Artifact | Min Lines | Actual Lines | Status | Details |
|----------|-----------|--------------|--------|---------|
| `app/slay3rd/Cargo.toml` | — | 47 | VERIFIED | All 5 commonware-* deps present; `{ workspace = true }` correctly declared |
| `app/slay3rd/src/main.rs` | 80 | 484 | VERIFIED | Full consensus entry point; loads config, BLS keys, creates LayerNode/LayerRelay, spawns Engine |
| `app/slay3rd/src/lib.rs` | 3 | 7 | VERIFIED | Declares: block, config, mempool, node, relay |
| `app/slay3rd/src/node.rs` | 100 | 618 | VERIFIED | LayerNode<T,P> CertifiableAutomaton; pending_payloads() accessor; execute_block(); 10 unit tests |
| `app/slay3rd/src/block.rs` | 20 | 94 | VERIFIED | BlockPayload with bincode serialization, SHA-256 digest, 3 unit tests |
| `app/slay3rd/src/mempool.rs` | 30 | 115 | VERIFIED | FIFO Mempool with 5 unit tests |
| `app/slay3rd/src/config.rs` | 40 | 135 | VERIFIED | NodeConfig with bls_key_path, from_file(), Default; 2 unit tests |
| `app/slay3rd/src/relay.rs` | 30 | 260 | VERIFIED | LayerRelay sharing pending_payloads Arc with LayerNode; receive_payload(); 4 unit tests |
| `packages/std/src/api/block.rs` | — | — | VERIFIED | `pub certificate: Option<Vec<u8>>` field present at line 45 |
| `tools/generate-testnet-keys/src/main.rs` | 40 | 237 | VERIFIED | bls12381 DKG via deal_anonymous::<MinSig, N3f1>; per-validator JSON output |
| `scripts/testnet.sh` | 30 | 291 | VERIFIED | start/stop/status/wait commands; 3-node config with ports 26656/26657/26658 |
| `scripts/verify-consensus.sh` | 20 | 329 | VERIFIED | AppHash comparison, SIGKILL crash recovery, certificate presence check, determinism audit |
| `tools/verify-cert/src/main.rs` | 20 | 348 | VERIFIED | bls12381 ops::verify_message::<MinSig>; --check-presence mode; fn main() present |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `Cargo.toml` | `app/slay3rd/Cargo.toml` | workspace members includes `"app/*"` | WIRED | Cargo.toml line 2: `members = ["app/*", "contracts/*", "packages/*"]` |
| `packages/app/src/bank/keeper.rs` | determinism | BTreeMap replaces HashMap | WIRED | Lines 2, 529, 536: BTreeMap used throughout; no HashMap present |
| `app/slay3rd/src/node.rs` | `packages/app/src/app.rs` | Arc<Mutex<App<T>>> in LayerNode fields | WIRED | Line 46: `app: Arc<Mutex<App<T>>>` |
| `app/slay3rd/src/node.rs` | `app/slay3rd/src/mempool.rs` | Arc<Mutex<Mempool>> in LayerNode fields | WIRED | Line 48: `mempool: Arc<Mutex<Mempool>>` |
| `app/slay3rd/src/node.rs` | `app/slay3rd/src/block.rs` | BlockPayload in propose() and certify() | WIRED | Lines 260 (construct), 296 (digest lookup in verify), 296 (remove in certify) |
| `app/slay3rd/src/relay.rs` | `app/slay3rd/src/node.rs` | Shares same pending_payloads Arc | WIRED | relay.rs line 49: `pending_payloads: Arc<...>`; main.rs line 276: `LayerRelay::new(layer_node.pending_payloads())` |
| `app/slay3rd/src/main.rs` | `app/slay3rd/src/node.rs` | Creates LayerNode and passes to consensus Config | WIRED | main.rs line 268: `LayerNode::new(...)`, line 432: `automaton: layer_node` |
| `app/slay3rd/src/main.rs` | `app/slay3rd/src/config.rs` | Loads NodeConfig to configure consensus | WIRED | main.rs line 155: `NodeConfig::from_file(&config_path)` |
| `tools/generate-testnet-keys/src/main.rs` | `app/slay3rd/src/config.rs` | Generates key material that NodeConfig loads | WIRED | Both reference bls12381 key format (bls_key_path -> keys.json) |
| `app/slay3rd/src/node.rs` | `packages/std/src/api/block.rs` | certify() stores BLS certificate in Block.certificate | NOT_WIRED | execute_block() sets `certificate: None` (line 166). Reporter receives cert bytes but only logs; does not write to Block. No code path sets `certificate: Some(cert_bytes)`. |

---

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| CONS-01 | 02-01, 02-02, 02-03 | CometBFT ABCI replaced with Commonware threshold_simplex Automaton | SATISFIED | LayerNode implements CertifiableAutomaton (node.rs); simplex Engine spawned in main.rs; old abci crate deleted in Phase 1 |
| CONS-02 | 02-02, 02-03, 02-04 | propose(), verify(), genesis() callbacks wired to App<T> | SATISFIED | genesis()->app.app_hash(), propose()->drain_batch()+finalize, verify()->pending_payloads lookup; all implemented and tested |
| CONS-03 | 02-03, 02-04 | Validator set via Commonware Scheme participants; crash recovery via WAL | SATISFIED (partial) | Participants Set built from Ed25519 keys in main.rs (line 305); WAL path in NodeConfig; crash recovery scripts exist. Live multi-node recovery not testable in Phase 2 (P2P limitation). |
| CONS-04 | 02-01, 02-02, 02-04 | Fully deterministic — no HashMap, SystemTime, floats in certify()/verify() paths | SATISFIED | BTreeMap everywhere in slay3rd; only comment references to HashMap/SystemTime; DETERMINISM-SAFE comment on cache.rs HashSet at cosmwasm_vm boundary |
| CONS-05 | 02-03, 02-04 | BLS12-381 threshold certificate per block, stored in block header | BLOCKED | Block.certificate field exists in struct; Reporter receives and logs cert bytes; but `certificate: None` is what gets stored in finalize_block(). Certificate is never actually written to the block header at runtime. |

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `app/slay3rd/src/node.rs` | 166 | `certificate: None` passed to finalize_block | BLOCKER | CONS-05 not achieved — certificate field in block header is always None |
| `app/slay3rd/src/main.rs` | 109-143 | LayerReporter::report() logs cert_bytes but does not persist them to Block | BLOCKER | The only code path that has the certificate bytes (the Reporter) does not write them to the committed block record |
| `app/slay3rd/src/main.rs` | 369-416 | commonware_p2p::simulated::Network cannot span OS processes | WARNING | 3-node testnet.sh scripts will fail silently — each process is its own isolated consensus network; this is documented as Phase 3 work |
| `app/slay3rd/src/node.rs` | 153 | `let txs: Vec<layer_std::Tx> = Vec::new();` | WARNING | Tx deserialization deferred — blocks are committed with empty tx list in Phase 2; documented limitation for Plan 04 |

---

### Human Verification Required

#### 1. Full workspace build

**Test:** Run `cargo build --workspace` from the repo root.
**Expected:** Exits 0 with all Commonware 2026.3.0 crates resolving correctly.
**Why human:** Dependency resolution (ahash version conflicts documented in SUMMARY) may vary by machine; needs confirmation.

#### 2. slay3rd unit tests

**Test:** Run `cargo test -p slay3rd` from the repo root.
**Expected:** All 18+ tests pass; no SIGBUS on macOS (sync #[test] + block_on pattern mitigates wasmer JIT race).
**Why human:** The SIGBUS fix is platform-specific; needs confirmation the sync test pattern works on the target machine.

#### 3. Keygen tool build

**Test:** Run `cargo build --manifest-path tools/generate-testnet-keys/Cargo.toml`.
**Expected:** Exits 0; then run the tool to generate actual key material files.
**Why human:** Standalone crate outside workspace; needs explicit build verification.

#### 4. verify-cert build

**Test:** Run `cargo build --manifest-path tools/verify-cert/Cargo.toml`.
**Expected:** Exits 0.
**Why human:** Standalone crate outside workspace.

---

### Gaps Summary

Two gaps block full phase goal achievement:

**Gap 1 — CONS-05: Certificate not stored in block header (blocker)**

The `Block.certificate` field was correctly added to `packages/std/src/api/block.rs` and all construction sites were updated with `certificate: None`. The Reporter (`LayerReporter` in main.rs) correctly receives the BLS12-381 certificate bytes from the Finalization activity and logs them. However, there is no code path that writes these bytes back into the Block that was already committed via `finalize_block()`. The `execute_block()` method in `node.rs` calls `finalize_block(block)` with `certificate: None`, and the Reporter fires afterwards — but it only logs, not stores.

The fix requires either: (a) restructuring the certify() flow to receive the certificate before or atomically with finalize_block(), or (b) adding an update API to App<T> to set the certificate for the last committed block by height after the Reporter delivers it.

**Gap 2 — CONS-03/CONS-02 (multi-node): Cross-process consensus deferred to Phase 3 (partial/known)**

The 3-node testnet scripts are complete, syntactically correct, and architecturally ready. However, because Phase 2 uses `commonware_p2p::simulated`, consensus messages cannot cross OS process boundaries. Each slay3rd process spawns its own isolated Network that cannot communicate with other processes. This is explicitly documented in STATE.md and the 02-04-SUMMARY.md as a Phase 3 prerequisite. The single-node consensus logic (LayerNode, CertifiableAutomaton, BlockPayload, Mempool, Relay) is fully correct and tested.

The Gap 1 (certificate storage) is the only blocker preventing the CONS-05 requirement from being met. Gap 2 is a known architectural deferral with explicit documentation and user approval at the Plan 04 human checkpoint.

---

*Verified: 2026-03-19T18:10:00Z*
*Verifier: Claude (gsd-verifier)*
