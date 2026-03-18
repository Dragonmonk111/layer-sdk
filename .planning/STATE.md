---
gsd_state_version: 1.0
milestone: v2.3
milestone_name: milestone
status: planning
stopped_at: Completed 01-02-PLAN.md
last_updated: "2026-03-18T00:00:00.000Z"
last_activity: 2026-03-18 — Plan 01-02 complete; CosmWasm v2.3.2 submodule integrated via patch.crates-io; BackendApi v2 + 14 Ethereum stubs; all cw-* upgraded to 2.0.0
progress:
  total_phases: 6
  completed_phases: 0
  total_plans: 3
  completed_plans: 2
  percent: 4
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-18)

**Core value:** WAVS programs can read from and write to persistent Layer state — enabling Layer to serve as the stateful backbone of the EigenLayer meta-chain ecosystem, with state anchored to Ethereum via zkVM proofs.
**Current focus:** Phase 1 — Foundation

## Current Position

Phase: 1 of 6 (Foundation)
Plan: 2 of 3 in current phase
Status: In progress
Last activity: 2026-03-18 — Plan 01-02 complete; CosmWasm v2.3.2 submodule integrated via patch.crates-io; BackendApi v2 + 14 Ethereum stubs; all cw-* upgraded to 2.0.0

Progress: [█░░░░░░░░░] 4%

## Performance Metrics

**Velocity:**
- Total plans completed: 2
- Average duration: ~120 min
- Total execution time: ~4 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-foundation | 2/3 | ~4h | ~120 min |

**Recent Trend:**
- Last 5 plans: 01-01 (workspace cleanup), 01-02 (CosmWasm v2 upgrade)
- Trend: On track

*Updated after each plan completion*
| Phase 01-foundation P01 | 6 | 2 tasks | 32 files |
| Phase 01-foundation P02 | 180 min | 3 tasks | 13 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: Commonware consensus (Phase 2) deliberately precedes Ethereum Types (Phase 3) — the ABCI layer can be replaced while the state machine still uses Cosmos types; this avoids a two-front migration
- [Roadmap]: Phase 2 (Consensus) depends on Phase 1 completing FOUND-02 (unsafe VM transmute) — hard prerequisite for safe async contexts
- [Roadmap]: Phase 4 (WASM Runtime) depends on Phase 3 (Ethereum Types) — CosmWasm VM replacement strips Cosmos types from the VM layer; the type migration must be complete first
- [Roadmap]: Phase 5 (WAVS) depends on both Phase 4 (WASM runtime) and Phase 2 (Commonware finalized blocks with BLS certificates)
- [Roadmap]: Phase 6 (zkVM) deferred to last — requires CONS-05 threshold certs, stable WASM runtime, and a Merkle state root not yet in the storage layer
- [Terminology]: "Ewasm" removed throughout — the old Ewasm project (ethereum/ewasm) is dead and its EEI spec is not being implemented; the correct framing is "de-Cosmos'd WASM runtime" (CosmWasm VM with Cosmos types stripped and Ethereum types substituted); host functions are idiomatic Ethereum-style, not Ewasm EEI
- [Phase 01-foundation]: cosmrs 0.13.0 transitively pulls in tendermint 0.31.1 via layer-cosmos/layer-golem — accepted as Phase 1 known acceptable (Phase 3 replaces cosmrs entirely)
- [Phase 01-foundation]: Upgraded tonic 0.12.2->0.12.3 and bytes 1.4.0->1.11.1 to fix addressable RUSTSEC advisories found during workspace audit
- [Plan 01-02]: patch.crates-io redirect pattern used for cosmwasm submodule — direct path deps cause nested workspace inheritance conflicts where outer workspace (0.5.0) resolves version instead of cosmwasm workspace (2.3.2)
- [Plan 01-02]: BackendApi Ethereum stubs use default unimplemented!() impls — VmApi compiles without changes; Phase 4 overrides with real linker registrations
- [Plan 01-02]: packages/golem excluded from workspace — cw-orch-core v1→v2 has 30+ breaking trait signature changes; deferred to separate migration task
- [Plan 01-02]: lib/cosmwasm uses upstream CosmWasm v2.3.2 as base (lay3r fork URL 404 at exec time) — lay3r-v2.3.2 branch created with Layer customizations applied

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 3]: Bech32 → H160 RocksDB key migration must be atomic with a `WriteBatch`; silent data corruption is the highest-risk failure in the project
- [Phase 2]: Commonware is ALPHA software (2026.3.0); API may change; mempool and block format are entirely application-defined with no framework guidance
- [Phase 6]: wreth architecture is undocumented publicly; must be clarified internally before Phase 6 planning can proceed

## Session Continuity

Last session: 2026-03-18T00:00:00.000Z
Stopped at: Completed 01-02-PLAN.md
Resume file: None
