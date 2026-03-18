---
phase: 01-foundation
plan: "01"
subsystem: infra
tags: [cargo, rust, workspace, tendermint, abci, security-audit]

# Dependency graph
requires: []
provides:
  - Clean Rust workspace building without CometBFT/Tendermint direct deps
  - packages/abci deleted (layer-abci)
  - app/slay3rd deleted (CometBFT application binary)
  - workspace Cargo.toml free of all tendermint-* declarations
affects: [02-consensus, 03-ethereum-types, 04-wasm-runtime]

# Tech tracking
tech-stack:
  added: [cargo-audit 0.22.1]
  patterns:
    - workspace-level dep upgrades for security advisories
    - transitive tendermint via cosmrs 0.13.0 accepted as Phase 3 concern

key-files:
  created: []
  modified:
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "Remove app/* from workspace members — slay3rd was the only app member; deleting it leaves an empty dir, not a problem"
  - "cosmrs 0.13.0 transitively pulls in tendermint 0.31.1 via layer-cosmos/layer-golem — accepted as Phase 1 known acceptable (Phase 3 replaces cosmrs)"
  - "Upgraded tonic 0.12.2->0.12.3 and bytes 1.4.0->1.11.1 to fix addressable RUSTSEC advisories"
  - "4 remaining RUSTSEC advisories (curve25519-dalek, idna, rkyv, time) are transitive-only, not fixable without major dep upgrades — deferred"

patterns-established:
  - "Workspace dependencies: never declare tendermint-* directly; prefer cosmwasm-* and cosmos-sdk-proto chains"

requirements-completed: [FOUND-01]

# Metrics
duration: 6min
completed: "2026-03-18"
---

# Phase 1 Plan 01: Remove Tendermint/ABCI Layer Summary

**Workspace cleaned of all direct CometBFT/Tendermint declarations: packages/abci and app/slay3rd deleted, 116 library tests passing, 2 RUSTSEC vulnerabilities fixed**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-18T14:46:27Z
- **Completed:** 2026-03-18T14:52:19Z
- **Tasks:** 2
- **Files modified:** 2 (Cargo.toml, Cargo.lock) + 30 deleted files

## Accomplishments

- Deleted `packages/abci/` (layer-abci crate: async ABCI server over tendermint-proto)
- Deleted `app/slay3rd/` (CometBFT application binary: tendermint, tendermint-abci, tendermint-rpc consumer)
- Removed `"app/*"` from workspace members, `layer-abci` from workspace deps, and all 4 `tendermint-*` declarations from `[workspace.dependencies]`
- Upgraded `tonic` 0.12.2 -> 0.12.3 (RUSTSEC-2024-0376: remotely exploitable DoS) and `bytes` 1.4.0 -> 1.11.1 (RUSTSEC-2026-0007: integer overflow) during audit task
- Confirmed tendermint is only transitive via `cosmrs 0.13.0 -> layer-cosmos -> layer-golem`

## Task Commits

Each task was committed atomically:

1. **Task 1: Delete ABCI and slay3rd packages, clean workspace deps** - `6972bb4` (feat)
2. **Task 2: Audit workspace and document transitive tendermint status** - `ac7ae19` (chore)

**Plan metadata:** (docs commit — see final_commit below)

## Files Created/Modified

- `Cargo.toml` - Removed `"app/*"` member, removed `layer-abci` dep, removed 4 `tendermint-*` deps, upgraded `bytes` and `tonic`
- `Cargo.lock` - Regenerated after dep changes
- `packages/abci/` - DELETED (6 files: Cargo.toml, README.md, src/{application,codec,error,lib,server}.rs)
- `app/slay3rd/` - DELETED (24 files: Cargo.toml, build.rs, config/, src/ including grpc/ subtree)

## Decisions Made

- **cosmrs transitive tendermint accepted**: `cosmrs 0.13.0` pulls in `tendermint 0.31.1` — this is Phase 3 work (replacing cosmrs with direct Ethereum signing). No action taken in Phase 1.
- **Security advisories fixed where possible**: Only `bytes` and `tonic` were directly in `[workspace.dependencies]`. The other 4 vulnerabilities (curve25519-dalek, idna, rkyv, time) are deep transitive deps locked by wasmer or cosmwasm-std — deferred to their respective major dependency upgrades.
- **`app/` directory kept empty**: No cleanup needed; cargo ignores empty glob patterns gracefully.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Fixed 2 RUSTSEC security advisories during Task 2 audit**
- **Found during:** Task 2 (audit workspace)
- **Issue:** cargo audit reported RUSTSEC-2026-0007 (bytes integer overflow) and RUSTSEC-2024-0376 (tonic DoS) — both were direct workspace dependencies with available fixes
- **Fix:** Upgraded `bytes = "1.4.0"` -> `"1.11.1"` and `tonic = "0.12.2"` -> `"0.12.3"` (and matching tonic-reflection, tonic-web) in workspace Cargo.toml
- **Files modified:** Cargo.toml, Cargo.lock
- **Verification:** cargo build --workspace and cargo test --workspace --lib both pass after upgrade; cargo audit now shows 4 (down from 6) vulnerabilities
- **Committed in:** ac7ae19 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (Rule 2 - missing critical security fixes found during mandated audit)
**Impact on plan:** Security fixes within the scope of the Task 2 audit action. No scope creep.

## Issues Encountered

- `tonic-web` and `tonic-reflection` both needed matching version bumps alongside `tonic` since they share the same release train
- 4 remaining RUSTSEC advisories (curve25519-dalek via ed25519-zebra, idna via url, rkyv via wasmer-types, time via tracing) have no workspace-level fix available and are documented as deferred

## User Setup Required

None - no external service configuration required.

## cargo audit Final Status

**4 vulnerabilities remain (all transitive, no workspace-level fix available):**

| Advisory | Crate | Path | Deferred Reason |
|---|---|---|---|
| RUSTSEC-2024-0344 | curve25519-dalek 3.2.0 | cosmwasm-crypto -> ed25519-zebra | Pinned by cosmwasm-std 1.5.x |
| RUSTSEC-2024-0421 | idna 0.5.0 | url crate chain | Transitive from url dep |
| RUSTSEC-2026-0001 | rkyv 0.7.45 | wasmer-types -> wasmer | Pinned by wasmer 4.2.x |
| RUSTSEC-2026-0009 | time 0.3.36 | tracing/parking_lot chain | Transitive |

**7 warnings (unmaintained crates):** derivative, fxhash, mach, paste, proc-macro-error, futures-util (yanked), tokio broadcast. All are transitive from wasmer or cosmwasm ecosystem.

**Transitive tendermint path:** `tendermint 0.31.1` <- `cosmrs 0.13.0` <- `layer-cosmos 0.5.0` <- `layer-golem 0.5.0` (Phase 3 will replace cosmrs)

## Next Phase Readiness

- Workspace builds cleanly: `cargo build --workspace` exits 0
- All library tests pass: `cargo test --workspace --lib` exits 0 (116 tests)
- FOUND-01 requirement met: workspace has no direct tendermint/CometBFT declarations
- Phase 2 (Commonware consensus integration) can proceed — no blockers from this plan

---
*Phase: 01-foundation*
*Completed: 2026-03-18*
