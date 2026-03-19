---
phase: 02-commonware-consensus
plan: "05"
subsystem: consensus
tags: [rust, bls12381, commonware, storage, certificate, reporter]

# Dependency graph
requires:
  - phase: 02-commonware-consensus
    provides: "LayerNode, LayerReporter, App<T> with finalize_block, and BLS simplex consensus bridge from plans 01-04"
provides:
  - "App::set_block_certificate(height, cert) — post-commit BLS certificate storage under _cert/{height} key"
  - "App::get_block_certificate(height) — certificate retrieval"
  - "LayerReporter holds Arc<Mutex<App<MemoryStore>>> and persists BLS certs on Finalization activity"
  - "CONS-05 gap closed: BLS certificates are now actually stored, not just logged"
affects: [03-ethereum-types, 06-zkvm]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Post-commit update pattern: App method separate from finalize_block for consensus metadata not available at block time"
    - "_-prefixed storage keys excluded from app_hash (LAST_BLOCK and certificate follow same convention)"
    - "Reporter-as-certificate-persister: consensus Reporter fires after quorum, correct injection point for cert storage"

key-files:
  created: []
  modified:
    - packages/app/src/app.rs
    - app/slay3rd/src/main.rs
    - app/slay3rd/src/node.rs

key-decisions:
  - "Certificate stored under '_cert/{height}' key with '_' prefix — excluded from app_hash, same as LAST_BLOCK, because certificate delivery timing is asynchronous and must not affect consensus determinism"
  - "Height determined from App::info().height at Reporter fire time — App's LAST_BLOCK is already incremented by certify()/execute_block(), so it reliably identifies the just-certified block"
  - "Item<Vec<u8>> with dynamic key from format!() is valid — Item borrows the local String for the duration of the method call; lifetime constraint is satisfied within the scope"
  - "LayerReporter changes from stateless (unit struct) to stateful (holds Arc<Mutex<App<T>>>) — Arc clone is cheap and the Reporter Clone bound is satisfied by Arc::clone"

patterns-established:
  - "Post-commit metadata pattern: consensus metadata unavailable at certify() time goes through Reporter; App provides a separate method for post-commit updates"
  - "Dynamic Item key pattern: Item<T>::new(&format_string) works for height-keyed storage when Item is created and used within the same scope"

requirements-completed: [CONS-05]

# Metrics
duration: 4min
completed: 2026-03-19
---

# Phase 2 Plan 05: CONS-05 Gap Closure Summary

**App::set_block_certificate() added and LayerReporter wired to persist BLS12-381 threshold certificates in storage keyed by block height on each Finalization activity**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-19T19:28:58Z
- **Completed:** 2026-03-19T19:32:21Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments

- Added `App::set_block_certificate(height, cert)` that writes BLS certificate bytes to persistent storage under `_cert/{height}` key (underscore prefix excluded from app_hash, matching LAST_BLOCK convention)
- Added `App::get_block_certificate(height)` for certificate retrieval, enabling downstream consumers (gRPC, zkVM proofs) to read stored certs
- Wired `LayerReporter` to hold `Arc<Mutex<App<MemoryStore>>>` and call `set_block_certificate()` on each `Finalization` activity, with structured logging on success/error
- Updated reporter construction in `run_node()` to pass `app_arc.clone()`
- Added `test_set_and_get_block_certificate` unit test proving the round-trip: init app, finalize block, store cert, retrieve cert, verify block 2 has no cert
- Updated `execute_block()` comment and tracing log to accurately reflect the new certificate storage flow

## Task Commits

Each task was committed atomically:

1. **Task 1: Add set_block_certificate() to App and wire LayerReporter to persist BLS certificates** - `2d6d9aa` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `packages/app/src/app.rs` — Added `BLOCK_CERTIFICATE_KEY_PREFIX` const, `set_block_certificate()` and `get_block_certificate()` methods, and `test_set_and_get_block_certificate` unit test
- `app/slay3rd/src/main.rs` — Changed `LayerReporter` from unit struct to struct with `app: Arc<Mutex<App<MemoryStore>>>` field; updated `report()` to call `set_block_certificate()` on Finalization; updated reporter construction in `run_node()`
- `app/slay3rd/src/node.rs` — Updated `execute_block()` comment (corrects old "relay.rs Plan 03" reference) and tracing log message to say "certificate stored by Reporter on Finalization activity"

## Decisions Made

- **Certificate storage key:** `_cert/{height}` — underscore prefix follows the same convention as `LAST_BLOCK` (`_last_block`), explicitly excluded from the Merkle app_hash since certificate delivery timing is asynchronous and must not affect consensus determinism across validators.
- **Height sourcing:** `app.info().map(|b| b.height).unwrap_or(0)` — at Reporter fire time, `execute_block()` has already called `finalize_block()` which increments LAST_BLOCK to the new height. `app.info()` reliably returns the just-committed block's height.
- **Dynamic key approach:** `Item::<Vec<u8>>::new(&key)` where `key = format!("{}{}", PREFIX, height)` — the `Item<'a, T>` borrows the string for `'a`, and since both `Item` and `key` are in the same scope, the lifetime is satisfied. This avoids introducing a Map type for what is essentially a height-indexed singleton.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None. The implementation matched the plan specification precisely:
- `Item::new()` accepts `&str` (which includes references to local `String` values within scope)
- `PulsarResult<()>` is the correct return type for storage write methods
- The `GasMeter::infinite()` and `reader.abort()` patterns from existing `finalize_block()` and `query()` methods applied directly

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CONS-05 is now closed: BLS certificates are stored in persistent storage after each block finalization
- `App::get_block_certificate(height)` is the retrieval API for Phase 3 (Ethereum Types) and Phase 6 (zkVM) when they need to read certificates for rollup proofs
- Phase 3 can proceed — all Phase 2 CONS-* requirements are satisfied

## Self-Check: PASSED

- FOUND: packages/app/src/app.rs
- FOUND: app/slay3rd/src/main.rs
- FOUND: app/slay3rd/src/node.rs
- FOUND: .planning/phases/02-commonware-consensus/02-05-SUMMARY.md
- FOUND: 2d6d9aa (feat(02-05): close CONS-05 gap — persist BLS certificates via LayerReporter)

---
*Phase: 02-commonware-consensus*
*Completed: 2026-03-19*
