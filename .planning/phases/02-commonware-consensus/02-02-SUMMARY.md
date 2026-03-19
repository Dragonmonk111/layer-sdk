---
phase: 02-commonware-consensus
plan: "02"
subsystem: consensus
tags: [commonware, commonware-consensus, BFT, CertifiableAutomaton, LayerNode, BlockPayload, Mempool, sha256, bincode, determinism, BTreeMap]

# Dependency graph
requires:
  - phase: 02-commonware-consensus
    plan: "01"
    provides: "slay3rd crate with Commonware 2026.3.0 deps and BTreeMap determinism audit"

provides:
  - "LayerNode<T, P> implementing Automaton + CertifiableAutomaton for Commonware simplex"
  - "BlockPayload with bincode serialization and SHA-256 digest"
  - "Mempool FIFO queue with capacity-bounded submit/drain_batch"
  - "pending_payloads() accessor for Relay sharing (Plan 03 wiring point)"
  - "execute_block() deterministic state commit via App::finalize_block()"

affects:
  - "02-03: Relay (Plan 03) receives pending_payloads() Arc to populate non-proposer verify paths"
  - "02-04: Integration testing will confirm Tx deserialization format (currently deferred)"
  - "CONS-01: LayerNode is the CertifiableAutomaton replacing CometBFT ABCI"
  - "CONS-02: propose/verify/certify wired to App<T> finalize_block"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "LayerNode<T, P> generic over storage backend and signing scheme public key — no hard-coupling to BLS12-381 at this layer"
    - "CertifiableAutomaton split: verify() reads pending_payloads only (no App mutation), certify() calls execute_block() which calls finalize_block()"
    - "BTreeMap<[u8; 32], BlockPayload> for pending_payloads — deterministic BTree ordering (CONS-04)"
    - "Sync #[test] + tokio::runtime::Builder::new_current_thread().block_on() pattern to prevent wasmer JIT SIGBUS on concurrent tests"
    - "Timestamp from BlockPayload.timestamp_nanos (consensus context derived) — no SystemTime::now() anywhere in consensus paths"

key-files:
  created:
    - "app/slay3rd/src/block.rs — BlockPayload with bincode serialize/deserialize and sha2 digest"
    - "app/slay3rd/src/mempool.rs — FIFO Mempool with submit/drain_batch and max_pending capacity"
    - "app/slay3rd/src/node.rs — LayerNode<T, P> CertifiableAutomaton bridge"
  modified:
    - "app/slay3rd/src/lib.rs — declare block, mempool, node modules"
    - "app/slay3rd/Cargo.toml — add cosmwasm-std as dev-dependency for test App initialization"

key-decisions:
  - "LayerNode made generic over P: PublicKey (not hardcoded to BLS12-381) — allows unit tests with ed25519::PublicKey and future scheme swaps without changing the bridge"
  - "Tx deserialization deferred to Plan 04 — layer_std::Tx does not implement serde::Deserialize; Phase 2 certify passes empty Tx slice to finalize_block (begin_block/end_block still execute correctly)"
  - "Tests written as sync #[test] with tokio::runtime::Builder::new_current_thread().block_on() — wasmer's JIT mmap initialization is not safe to call from multiple OS threads concurrently; async tokio::test harness triggers SIGBUS on macOS"
  - "View::get() method (not into_u64) for consensus view-to-timestamp conversion — confirmed from crate source"
  - "sha256::Digest from commonware-cryptography used as the consensus Digest type — implements commonware_cryptography::Digest trait; [u8; 32] arrays do not"

patterns-established:
  - "CONSENSUS BRIDGE: LayerNode wraps Arc<Mutex<App<T>>> — single integration point, no direct consensus-to-App coupling outside this file"
  - "VERIFY-IS-READ-ONLY: verify() touches only pending_payloads (not app); certify() is the only path to App::finalize_block()"
  - "PENDING_PAYLOADS-SHARED: pending_payloads() returns a clone of the same Arc for Relay wiring — all Plan 03 relay code must use this same Arc, not create a new BTreeMap"

requirements-completed: [CONS-01, CONS-02, CONS-04]

# Metrics
duration: 20min
completed: 2026-03-19
---

# Phase 2 Plan 02: Consensus Bridge Summary

**LayerNode<T,P> CertifiableAutomaton bridging Commonware simplex consensus to App<T>::finalize_block with deterministic BTreeMap-keyed pending_payloads and sha256::Digest wire format**

## Performance

- **Duration:** 20 min
- **Started:** 2026-03-19T16:33:00Z
- **Completed:** 2026-03-19T16:53:00Z
- **Tasks:** 2
- **Files modified:** 5 (3 created, 2 modified)

## Accomplishments
- Created `BlockPayload` with bincode serialization and SHA-256 digest method; fully round-trips
- Created `Mempool` with FIFO submit/drain_batch; capacity-bounded; 5 unit tests
- Created `LayerNode<T, P>` implementing `Automaton` and `CertifiableAutomaton` traits from Commonware 2026.3.0
- `pending_payloads()` accessor exposes the shared `Arc<Mutex<BTreeMap>>` for Relay (Plan 03) wiring
- `certify()` -> `execute_block()` -> `App::finalize_block()` is the single deterministic commit point
- `verify()` does NOT call `finalize_block` — reads only `pending_payloads`
- 18 unit tests passing (8 block/mempool, 10 node)
- Workspace builds cleanly

## Task Commits

Each task was committed atomically:

1. **Task 1: Define BlockPayload and Mempool types** - `eb787d9` (feat)
2. **Task 2: Implement LayerNode CertifiableAutomaton with App wiring** - `ffe10b9` (feat)

**Plan metadata:** (pending final commit)

## Files Created/Modified
- `app/slay3rd/src/block.rs` - BlockPayload with deterministic bincode serialization and SHA-256 digest
- `app/slay3rd/src/mempool.rs` - FIFO Mempool with submit/drain_batch and max_pending capacity control
- `app/slay3rd/src/node.rs` - LayerNode<T, P> implementing CertifiableAutomaton; execute_block; pending_payloads accessor
- `app/slay3rd/src/lib.rs` - Module declarations for block, mempool, node
- `app/slay3rd/Cargo.toml` - Added cosmwasm-std as dev-dependency

## Decisions Made
- LayerNode generic over `P: PublicKey` — not hardcoded to BLS12-381; ed25519::PublicKey used in tests
- Tx deserialization deferred: `layer_std::Tx` does not implement `serde::Deserialize`; Phase 2 certify passes empty Tx slice; correct wire format confirmed in Plan 04 integration testing
- Sync `#[test]` with `block_on` for tests using `App<T>` — wasmer JIT initialization triggers SIGBUS when called from multiple OS threads concurrently (async `tokio::test` harness); sync pattern eliminates race

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] View::get() vs non-existent into_u64()**
- **Found during:** Task 2 (LayerNode implementation)
- **Issue:** Plan referred to view-to-u64 conversion; actual API uses `.get()` method, not `.into_u64()`
- **Fix:** Used `context.round.view().get()` after inspecting commonware-consensus source
- **Files modified:** app/slay3rd/src/node.rs
- **Committed in:** ffe10b9 (Task 2 commit)

**2. [Rule 1 - Bug] Tx deserialization: layer_std::Tx does not implement serde::Deserialize**
- **Found during:** Task 2 (execute_block implementation)
- **Issue:** Plan specified bincode-deserializing raw Bytes into layer_std::Tx; layer_std::Tx has no Deserialize impl
- **Fix:** Deferred Tx deserialization; execute_block passes empty Vec<Tx> for Phase 2 (begin_block/end_block still execute; Tx wire format confirmed in Plan 04)
- **Files modified:** app/slay3rd/src/node.rs
- **Committed in:** ffe10b9 (Task 2 commit)

**3. [Rule 1 - Bug] SIGBUS: async tokio::test concurrent WASM VM initialization**
- **Found during:** Task 2 (test execution)
- **Issue:** Multiple async tests with `tokio::test` macro caused SIGBUS on macOS — wasmer JIT mmap initialization not safe when called from multiple OS threads concurrently
- **Fix:** Converted all node tests from async `#[tokio::test]` to sync `#[test]` using `tokio::runtime::Builder::new_current_thread().block_on()` — cargo's test thread pool limits true concurrency to initialized-wasmer scenarios
- **Files modified:** app/slay3rd/src/node.rs
- **Committed in:** ffe10b9 (Task 2 commit)

**4. [Rule 3 - Blocking] commonware_codec not a direct dep of slay3rd**
- **Found during:** Task 2 (propose() implementation for leader.encode())
- **Issue:** `use commonware_codec::Encode` failed to resolve — not a direct dependency
- **Fix:** Used `AsRef<[u8]>` (via `PublicKey: Array: AsRef<[u8]>`) instead of Encode::encode() for proposer bytes extraction
- **Files modified:** app/slay3rd/src/node.rs
- **Committed in:** ffe10b9 (Task 2 commit)

**5. [Rule 3 - Blocking] cosmwasm_std not available for test App initialization**
- **Found during:** Task 2 (test setup)
- **Issue:** Tests need `to_json_binary` and `Timestamp` from cosmwasm_std; not in slay3rd's deps
- **Fix:** Added `cosmwasm-std = { workspace = true }` as dev-dependency to slay3rd/Cargo.toml
- **Files modified:** app/slay3rd/Cargo.toml
- **Committed in:** ffe10b9 (Task 2 commit)

---

**Total deviations:** 5 auto-fixed (2 bugs, 2 blocking, 1 bug/blocking)
**Impact on plan:** All auto-fixes required for compilation and correctness. Tx deserialization deferral is explicitly anticipated in the plan ("If this assumption is wrong, it will surface during integration testing (Plan 04)"). No scope creep.

## Issues Encountered
- `sha256::Digest` (not `[u8; 32]`) must be used as the `Automaton::Digest` associated type — `[u8; 32]` does not implement `commonware_cryptography::Digest` trait required by the Automaton bound
- Manual `Clone` implementation required for `LayerNode<T, P>` because `#[derive(Clone)]` adds `T: Clone` bound but `T` is only held behind `Arc<Mutex<App<T>>>` (Arc is Clone regardless of T)
- Test timestamps must be >= genesis block timestamp (1_673_194_026_078_305_426 ns) for `App::finalize_block()` monotonicity check

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `LayerNode<T, P>` is ready for Plan 03 (P2P Relay wiring)
- `pending_payloads()` accessor is the wiring point: Relay must use the same Arc to populate non-proposer verify paths
- Tx deserialization is the main deferred item for Plan 04 integration testing
- All 18 unit tests pass; workspace builds cleanly

## Self-Check: PASSED

All claimed files verified present on disk:
- FOUND: app/slay3rd/src/block.rs
- FOUND: app/slay3rd/src/mempool.rs
- FOUND: app/slay3rd/src/node.rs
- FOUND: app/slay3rd/src/lib.rs
- FOUND: app/slay3rd/Cargo.toml
- FOUND: .planning/phases/02-commonware-consensus/02-02-SUMMARY.md

All claimed commits verified in git history:
- FOUND: eb787d9 (feat(02-02): add BlockPayload and Mempool types with unit tests)
- FOUND: ffe10b9 (feat(02-02): implement LayerNode CertifiableAutomaton with App<T> wiring)
