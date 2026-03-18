# Phase 1: Foundation - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Clean the codebase foundation so all subsequent phases can build on it: remove all CometBFT/Tendermint dependencies from the workspace, fix the unsafe WASM VM transmute, fix contract address generation determinism, and fork CosmWasm 2.3.2 as a git submodule with the CWA-2024-004 security fix and stub Ethereum host function signatures. This phase delivers a buildable workspace — not a runnable node.

</domain>

<decisions>
## Implementation Decisions

### CometBFT / Tendermint Removal

- **Breaking changes are fine** — no backwards compatibility required. This is a major refactor.
- Delete `packages/abci/` entirely. It is the CometBFT integration layer and must be fully removed.
- The `slay3rd` binary (or `app/slay3rd/`) can also be deleted or gutted — Phase 2 creates a new Commonware-based entry point from scratch.
- The workspace goal for Phase 1 is **`cargo build` and `cargo test` pass** on all remaining library crates. A runnable node is NOT required.
- Where library crates (`packages/app`, `packages/cosmos`, etc.) import Tendermint types directly, replace them with equivalent `std` or `alloy_primitives` types (e.g., block timestamps → `u64` Unix seconds, hashes → `[u8; 32]`). This prepares for Phase 3's Ethereum type migration.
- No placeholder newtype wrappers — prefer real type replacements.

### CosmWasm Fork

- The Lay3r fork of CosmWasm is **created manually** by the team (out-of-band). Phase 1 does not create the GitHub fork — it adds the submodule pointing to the already-existing fork at the correct commit.
- Target: fork based on CosmWasm v2.3.2.
- **What goes into the fork (Phase 1 scope):**
  1. Cherry-pick CWA-2024-004 gas mispricing security fix (FORK-01 requirement)
  2. `BackendApi` extensibility hook for custom host functions (FORK-02 requirement)
  3. Stub Ethereum host function signatures in `BackendApi`: `storageLoad`, `storageStore`, `getCaller`, `getCallValue`, `getBlockNumber`, `getBlockTimestamp`, `useGas`, `finish`, `revert`, `log0`–`log4` — unimplemented but present as trait methods with default panics. Reduces Phase 4's diff on the fork.
- **All cosmwasm-* crates** come from the fork submodule: `cosmwasm-vm`, `cosmwasm-std`, `cosmwasm-schema`, `cosmwasm-crypto`. The workspace Cargo.toml switches all cosmwasm registry dependencies to path/git dependencies pointing to the submodule.

### Claude's Discretion

- `danger_will_robinson` unsafe transmute fix (FOUND-02) — Claude chooses the approach. Preferred direction: restructure ownership to avoid lifetime transmutation entirely; favor approaches that set up Phase 2's async integration cleanly. Arc/Mutex is acceptable if restructuring is too invasive.
- WASM contract address generation fix (FOUND-03) — Claude determines what the actual non-determinism is and fixes it. The success criteria is: same deployer + salt produces the same address on every node, with no `SystemTime` or random input. If the counter-based v1 approach is already deterministic, prove it with a test; if not, migrate to CREATE2-style.
- Cargo dependency upgrade strategy — Claude determines the order for resolving 2-year-old dependency conflicts. No approach preference from the user.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 1 requirements and success criteria
- `.planning/ROADMAP.md` — Phase 1 goal, success criteria for FOUND-01, FOUND-02, FOUND-03, FORK-01, FORK-02
- `.planning/REQUIREMENTS.md` — Full requirement specifications for Foundation and CosmWasm Fork sections

### Existing unsafe code (fix target)
- `packages/app/src/wasm/vm/backend.rs` — `danger_will_robinson` function with `mem::transmute` (FOUND-02 fix target); `VmQuerier` and `VmStore` with `'static` references
- `packages/app/src/wasm/vm/cache.rs` — 6 call sites for `danger_will_robinson`
- `packages/app/src/wasm/utils.rs` — `build_instantiate_address` (counter-based v1) and `build_instantiate_2_address` (CREATE2-style)
- `packages/app/src/wasm/keeper.rs` — `generate_address` using contract counter (FOUND-03 fix target)

### CometBFT integration (deletion targets)
- `packages/abci/` — entire CometBFT ABCI server package (delete)
- `Cargo.toml` — workspace dependency declarations for `tendermint`, `tendermint-abci`, `tendermint-proto`, `tendermint-rpc` (remove)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `packages/app/src/wasm/vm/backend.rs` — `VmApi`, `VmStore`, `VmQuerier` implementations for the CosmWasm VM backend; core logic stays, lifetime management changes
- `packages/app/src/wasm/utils.rs` — `build_instantiate_2_address` (CREATE2-style, deterministic) already exists and is correct; may be the model for fixing v1
- `packages/app/src/wasm/vm/cache.rs` — `WasmCache` orchestration layer; calls `danger_will_robinson` 6× (all call sites need updating when the transmute is fixed)

### Established Patterns
- Current stack: CosmWasm 1.5.4, tendermint 0.39.1, CometBFT ABCI via `packages/abci/`
- The workspace uses `resolver = "2"` and a centralized `[workspace.dependencies]` table — dependency version changes go in one place
- `packages/app/` is the framework-agnostic state machine; it must remain independent of the consensus layer after Phase 1

### Integration Points
- After Phase 1: `packages/app/` is the only entry point for Phase 2's Commonware integration — the Automaton callbacks wire into `App<T>`
- CosmWasm submodule will be at `cosmwasm/` or `deps/cosmwasm/` in the repo root; `Cargo.toml` workspace deps switch from registry to `{ path = "cosmwasm/packages/..." }` or `{ git = "...", tag = "..." }`

</code_context>

<specifics>
## Specific Ideas

- User explicitly said: "I'm fine if we make breaking changes. This is a major refactor and backwards compatibility is not needed."
- The end state of Phase 1 is a **buildable workspace**, not a runnable node. The node comes back in Phase 2 with Commonware.
- Ethereum host function signatures in the fork should be stubs with `unimplemented!()` or `todo!()` — their presence matters for Phase 4, their implementation does not.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-foundation*
*Context gathered: 2026-03-18*
