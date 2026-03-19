---
phase: 02-commonware-consensus
plan: "01"
subsystem: consensus
tags: [commonware, commonware-consensus, commonware-p2p, commonware-cryptography, commonware-runtime, commonware-storage, determinism, BTreeMap, BTreeSet, cargo-workspace]

# Dependency graph
requires:
  - phase: 01-foundation
    provides: "Cleaned workspace, CosmWasm v2 upgrade, unsafe transmute elimination — all prerequisites for safe async contexts"
provides:
  - "slay3rd binary crate at app/slay3rd/ with all 5 commonware-* 2026.3.0 dependencies"
  - "Workspace member app/* added to Cargo.toml"
  - "Commonware 2026.3.0 workspace dependency declarations"
  - "HashMap-free consensus-critical code paths in packages/app/src/"
affects:
  - "02-02: LayerNode Automaton implementation uses slay3rd crate scaffolded here"
  - "02-03: BLS DKG and threshold signing builds on this crate foundation"
  - "CONS-04: determinism audit complete; certify/verify paths are HashMap-free"

# Tech tracking
tech-stack:
  added:
    - "commonware-consensus 2026.3.0"
    - "commonware-p2p 2026.3.0"
    - "commonware-cryptography 2026.3.0"
    - "commonware-runtime 2026.3.0"
    - "commonware-storage 2026.3.0"
    - "tracing-subscriber 0.3.17 (workspace dep)"
    - "bincode 1.3 (workspace dep)"
  patterns:
    - "Commonware crates declared as workspace deps at 2026.3.0 — version pinned for ALPHA stability"
    - "BTreeMap/BTreeSet replaces HashMap/HashSet in all consensus-critical App<T> code paths"
    - "DETERMINISM-SAFE comment pattern for HashSet retained at cosmwasm_vm API boundary"

key-files:
  created:
    - "app/slay3rd/Cargo.toml — slay3rd binary crate with all 5 commonware-* deps"
    - "app/slay3rd/src/main.rs — Phase 2 scaffold entry point"
    - "app/slay3rd/src/lib.rs — Library root, module structure added in Plan 02-02"
  modified:
    - "Cargo.toml — added app/* to members, Commonware 2026.3.0 workspace deps"
    - "packages/app/src/bank/keeper.rs — HashMap -> BTreeMap in ValidCoins.seen"
    - "packages/app/src/wasm/vm/backend.rs — HashMap -> BTreeMap in VmStore.iterators"
    - "packages/app/src/wasm/vm/cache.rs — DETERMINISM-SAFE comment on capabilities() HashSet"

key-decisions:
  - "Commonware 2026.3.0 pinned exactly — ALPHA software; API may change; exact version confirmed via crates.io API on 2026-03-19"
  - "BTreeMap replaces HashMap in VmStore.iterators (backend.rs) — iterators are keyed by sequential u32 IDs; BTreeMap has same correctness semantics and ensures deterministic iteration if ever traversed"
  - "HashSet retained in capabilities() (cache.rs) at cosmwasm_vm CacheOptions boundary — cosmwasm_vm requires impl Into<HashSet<String>>; BTreeSet does not implement this conversion; capabilities() is only called at VM instantiation, not in certify/verify paths"
  - "bincode added as workspace dep — deterministic binary serialization for BlockPayload encoding in Plan 02-02"

patterns-established:
  - "DETERMINISM-SAFE comment pattern: HashSet/HashMap retained at external API boundaries must have a comment explaining why it is safe (called only at structural validation, not in consensus paths)"
  - "All new BTreeMap usages in consensus code: no HashMap permitted in paths reachable from App::finalize_block()"

requirements-completed: [CONS-01, CONS-04]

# Metrics
duration: 6min
completed: 2026-03-19
---

# Phase 2 Plan 01: Commonware Foundation Summary

**slay3rd binary crate scaffolded with 5 Commonware 2026.3.0 deps; HashMap replaced with BTreeMap in all App<T> consensus-critical code paths**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-19T16:22:07Z
- **Completed:** 2026-03-19T16:28:07Z
- **Tasks:** 2
- **Files modified:** 6 (3 created, 3 modified)

## Accomplishments
- Created `app/slay3rd` binary crate with all 5 commonware-* crates at 2026.3.0 as workspace dependencies
- Added `app/*` to workspace members and Commonware + tracing-subscriber + bincode to workspace deps
- Replaced all `HashMap`/`HashSet` in consensus-critical paths with `BTreeMap`/`BTreeSet`; only one `HashSet` remains (in `capabilities()` with a DETERMINISM-SAFE comment)
- Workspace builds cleanly; all 113 existing tests pass after changes

## Task Commits

Each task was committed atomically:

1. **Task 1: Create slay3rd binary crate with Commonware dependencies** - `eec17bd` (feat)
2. **Task 2: Determinism audit — replace HashMap/HashSet** - `3c0a661` (fix)

**Plan metadata:** (pending final commit)

## Files Created/Modified
- `app/slay3rd/Cargo.toml` - slay3rd binary crate with 5 commonware-* deps, layer-* deps, tokio/tracing/bincode
- `app/slay3rd/src/main.rs` - Placeholder binary entry point (Phase 2 scaffold)
- `app/slay3rd/src/lib.rs` - Library root (module structure added in Plan 02-02)
- `Cargo.toml` - Added app/* to workspace members; Commonware 2026.3.0 workspace deps; tracing-subscriber; bincode
- `packages/app/src/bank/keeper.rs` - ValidCoins.seen: HashMap -> BTreeMap
- `packages/app/src/wasm/vm/backend.rs` - VmStore.iterators: HashMap -> BTreeMap
- `packages/app/src/wasm/vm/cache.rs` - capabilities(): DETERMINISM-SAFE comment added for HashSet at cosmwasm_vm boundary

## Decisions Made
- Commonware 2026.3.0 pinned exactly (ALPHA software; version confirmed via crates.io API)
- BTreeMap replaces HashMap in VmStore.iterators — same semantics for u32-keyed iterators, deterministic if traversed
- HashSet retained in capabilities() only at the cosmwasm_vm `CacheOptions::new()` API boundary (requires `impl Into<HashSet<String>>`; BTreeSet does not satisfy this); capabilities() is only called during VM instantiation, not in certify/verify paths

## Deviations from Plan

None — plan executed exactly as written.

The cache.rs `capabilities()` handling (keeping HashSet with DETERMINISM-SAFE comment) was the explicit fallback path specified in the plan when cosmwasm_vm requires `HashSet<String>`.

## Issues Encountered
- `BTreeSet<String>` does not implement `Into<HashSet<String>>` — confirmed by attempting a type conversion. The plan explicitly anticipated this case and specified the DETERMINISM-SAFE comment fallback.
- ahash has two versions in the dep tree (0.7.8 for wasmer/indexmap, 0.8.12 for commonware) — this is expected and harmless; each dep tree resolves independently; workspace builds cleanly.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- slay3rd crate is ready for Plan 02-02 which adds `node.rs` (LayerNode implementing CertifiableAutomaton), `mempool.rs`, `relay.rs`, `block.rs`
- All HashMap/HashSet in consensus-critical paths eliminated; CONS-04 determinism prerequisite satisfied
- Workspace builds and all tests pass — clean baseline for consensus implementation

## Self-Check: PASSED

- app/slay3rd/Cargo.toml — FOUND
- app/slay3rd/src/main.rs — FOUND
- app/slay3rd/src/lib.rs — FOUND
- 02-01-SUMMARY.md — FOUND
- Commit eec17bd (Task 1) — FOUND
- Commit 3c0a661 (Task 2) — FOUND
- commonware-consensus = { workspace = true } in slay3rd/Cargo.toml — FOUND
- BTreeMap in bank/keeper.rs — FOUND
- BTreeMap in backend.rs — FOUND
- DETERMINISM-SAFE in cache.rs — FOUND
- app/* in workspace members — FOUND

---
*Phase: 02-commonware-consensus*
*Completed: 2026-03-19*
