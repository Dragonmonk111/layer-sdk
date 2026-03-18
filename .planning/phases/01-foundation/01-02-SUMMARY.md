---
phase: 01-foundation
plan: 02
subsystem: infra
tags: [cosmwasm, wasm, cargo, git-submodule, backendapi, ethereum, gas, checksum]

# Dependency graph
requires:
  - phase: 01-01
    provides: clean workspace without ABCI/dead packages

provides:
  - CosmWasm v2.3.2 fork as git submodule at lib/cosmwasm with CWA-2024-004 gas fix
  - BackendApi v2 interface (addr_validate, addr_canonicalize, addr_humanize) in VmApi
  - 14 Ethereum host function stubs in fork BackendApi trait
  - cosmwasm_2_0 capability enabled; SDK_TO_WASMER_GAS_FACTOR at 150_000
  - All cw-* ecosystem crates upgraded to 2.0.0

affects: [02-evm-runtime, 03-evm-execution, 04-eth-host-functions, any phase using cosmwasm-std or cosmwasm-vm]

# Tech tracking
tech-stack:
  added:
    - cosmwasm-vm 2.3.2 (via path dep, patch.crates-io)
    - cosmwasm-std 2.3.2 (via path dep, patch.crates-io)
    - cosmwasm-schema/crypto/core 2.3.2 (via patch.crates-io)
    - cw-storage-plus 2.0.0 (upgraded from 1.2.0)
    - cw-utils/cw-multi-test/cw2/cw20/cw20-base 2.0.0 (upgraded from 1.x)
    - cw-orch-core 2 / abstract-cw-multi-test 2 (upgraded from 1.x)
  patterns:
    - patch.crates-io redirect pattern for nested Cargo workspace submodules
    - BackendApi trait extension via default-impl stubs for future phases
    - Hardcoded package versions in submodule to avoid workspace inheritance conflicts

key-files:
  created:
    - lib/cosmwasm/ (git submodule at commit 49178c9f6, lay3r-v2.3.2 branch)
  modified:
    - Cargo.toml (workspace deps switched to patch.crates-io, cw-* upgraded to 2.x)
    - lib/cosmwasm/packages/vm/src/backend.rs (14 Ethereum host function stubs)
    - lib/cosmwasm/packages/*/Cargo.toml (hardcoded version = "2.3.2" in all 8 packages)
    - packages/app/src/wasm/vm/backend.rs (BackendApi v2 interface, addr_validate added)
    - packages/app/src/wasm/vm/cache.rs (gas factor 150_000, cosmwasm_2_0 capability)
    - packages/app/src/wasm/utils.rs (cosmwasm_std::Checksum replaces Binary alias)
    - packages/app/src/wasm/keeper.rs (SubMsgResponse, Reply, Checksum v2 fields)
    - packages/std/src/account_id.rs (KEY_ELEMS: u16 = 1 uncommented)
    - packages/std/src/query.rs (ContractInfoResponse::new() constructor)
    - packages/storage/src/plus/item.rs (OverflowError::new() 1-arg form)
    - contracts/caller/src/contract.rs (SubMsg payload field added)

key-decisions:
  - "Use [patch.crates-io] to redirect cosmwasm crates to submodule rather than direct path deps — avoids nested workspace inheritance conflicts"
  - "Hardcode version = '2.3.2' in submodule package Cargo.toml files — required because outer workspace resolves workspace = true against layer-sdk (0.5.0) not cosmwasm (2.3.2)"
  - "Exclude packages/golem from workspace — cw-orch-core v1 to v2 has breaking trait signature changes; deferred to separate migration"
  - "Ethereum host function stubs use unimplemented!() with default impl — preserves VmApi backward compat; Phase 4 overrides with real implementations"
  - "lib/cosmwasm points to upstream CosmWasm v2.3.2 on lay3r-v2.3.2 branch — fork URL (lay3r-labs/cosmwasm) was 404 at execution time; upstream used as base"

patterns-established:
  - "Pattern 1: patch.crates-io for submodule crates — when patching a nested workspace crate, use [patch.crates-io] with explicit version pins, not direct path deps"
  - "Pattern 2: BackendApi stub extension — add default-impl stubs to BackendApi trait to expose future capabilities without breaking existing impls"
  - "Pattern 3: Submodule version hardcoding — nested submodule packages must hardcode their version strings to decouple from outer workspace"

requirements-completed: [FORK-01, FORK-02]

# Metrics
duration: 180min
completed: 2026-03-18
---

# Phase 1 Plan 2: CosmWasm Fork Integration Summary

**CosmWasm v2.3.2 fork submodule with BackendApi v2 (addr_validate), 14 Ethereum host function stubs (storage_load/store, get_caller, log0-4), and full cw-* 2.x ecosystem upgrade via patch.crates-io redirect pattern**

## Performance

- **Duration:** ~180 min (across two sessions)
- **Started:** 2026-03-17T00:00:00Z (approx)
- **Completed:** 2026-03-18T00:00:00Z (approx)
- **Tasks:** 3 completed
- **Files modified:** 13 (plus 8 submodule Cargo.toml files)

## Accomplishments

- Added CosmWasm v2.3.2 as a git submodule at `lib/cosmwasm` on branch `lay3r-v2.3.2`; verified CWA-2024-004 gas mispricing fix present (GAS_PER_OPERATION=115 with 14x control-flow multiplier)
- Upgraded entire workspace from cosmwasm-std 1.5.4 to 2.3.2 using `[patch.crates-io]` pattern; all cw-* ecosystem crates (storage-plus, utils, multi-test, cw2, cw20, cw20-base) upgraded to 2.0.0
- Implemented BackendApi v2 in VmApi: renamed canonical_address/human_address to addr_canonicalize/addr_humanize, added addr_validate; updated gas factor (150_000_000 → 150_000), enabled cosmwasm_2_0 capability
- Added 14 Ethereum host function stubs to BackendApi trait in fork: storage_load, storage_store, get_caller, get_call_value, get_block_number, get_block_timestamp, use_gas, finish, revert, log0-log4; all 112 workspace tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add CosmWasm fork submodule and switch workspace deps** - `6b36802` (chore)
2. **Task 2: Upgrade BackendApi v1 to v2 and update VM constants** - `4bb5ad2` (feat)
3. **Task 3: Add Ethereum host function stubs to fork BackendApi** - `6e24baf` (feat)

## Files Created/Modified

- `lib/cosmwasm/` - Git submodule at lay3r-v2.3.2 branch (commit 49178c9f6)
- `lib/cosmwasm/packages/vm/src/backend.rs` - 14 Ethereum host function stubs added to BackendApi trait
- `lib/cosmwasm/packages/*/Cargo.toml` - All 8 packages hardcoded to version = "2.3.2"
- `Cargo.toml` - workspace.dependencies uses =2.3.2 pins + [patch.crates-io] redirects; cw-* upgraded to 2.0.0; packages/golem excluded
- `packages/app/src/wasm/vm/backend.rs` - BackendApi v2 (addr_validate/addr_canonicalize/addr_humanize); GAS_COST_VALIDATE_ADDRESS uncommented
- `packages/app/src/wasm/vm/cache.rs` - SDK_TO_WASMER_GAS_FACTOR=150_000; cosmwasm_2_0 capability; store_code() API; Checksum import from cosmwasm_std
- `packages/app/src/wasm/utils.rs` - cosmwasm_std::Checksum replaces Binary type alias; Instantiate2AddressError simplified
- `packages/app/src/wasm/keeper.rs` - SubMsgResponse msg_responses field; Reply gas_used/payload fields; Checksum conversion from Binary
- `packages/std/src/account_id.rs` - KEY_ELEMS: u16 = 1 uncommented in both KeyDeserialize impls
- `packages/std/src/query.rs` - ContractInfoResponse::new() constructor in From impl
- `packages/storage/src/plus/item.rs` - OverflowError::new() 1-arg form (v2 API change)
- `contracts/caller/src/contract.rs` - SubMsg payload: Binary::default() field added

## Decisions Made

- **patch.crates-io redirect pattern:** Used `[patch.crates-io]` with `version = "=2.3.2"` pins instead of direct path deps. This is necessary because direct path deps from an outer workspace inherit `workspace = true` metadata from the outer workspace (version 0.5.0), causing the patched versions to show as 0.5.0 not 2.3.2. The patch mechanism bypasses this by first resolving against crates.io registry (at =2.3.2) then redirecting to the local path.

- **Submodule package version hardcoding:** The cosmwasm package Cargo.toml files use `version = { workspace = true }` which resolves to `0.5.0` from the outer workspace. All 8 packages were patched to use explicit `version = "2.3.2"` on the `lay3r-v2.3.2` branch.

- **packages/golem exclusion:** cw-orch-core v1 → v2 migration has breaking trait signature changes (30+ compilation errors). Deferred to a separate migration task; golem excluded from workspace members to unblock the rest.

- **Upstream CosmWasm as fork base:** The Lay3r fork URL (https://github.com/lay3r-labs/cosmwasm) returned 404 at execution time. The existing `lib/cosmwasm` submodule (upstream CosmWasm at v2.3.2) was used as the base. The `lay3r-v2.3.2` branch was created on this base with Layer customizations.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Lay3r CosmWasm fork URL 404 - used upstream as base**
- **Found during:** Task 1 (submodule setup)
- **Issue:** `https://github.com/lay3r-labs/cosmwasm` returned 404; existing `lib/cosmwasm` pointed to upstream at v2.3.2
- **Fix:** Created `lay3r-v2.3.2` branch on upstream base; applied all customizations there
- **Files modified:** lib/cosmwasm/ (submodule pointer)
- **Verification:** `git submodule status` shows commit 49178c9f6
- **Committed in:** 6b36802 (Task 1)

**2. [Rule 3 - Blocking] Nested Cargo workspace inheritance conflict**
- **Found during:** Task 1 (cargo build)
- **Issue:** cosmwasm packages use `workspace = true` for version/edition/repository/license AND for cross-package deps; when referenced as path deps from layer-sdk, the outer workspace (0.5.0) resolves these instead of the cosmwasm workspace (2.3.2)
- **Fix:** Hardcoded `version = "2.3.2"` in all 8 cosmwasm package Cargo.toml files; used `[patch.crates-io]` with `=2.3.2` pins in layer-sdk workspace
- **Files modified:** lib/cosmwasm/packages/*/Cargo.toml (8 files), Cargo.toml
- **Verification:** `cargo build --workspace` exits 0
- **Committed in:** 6b36802 (Task 1, submodule commit 49178c9f6)

**3. [Rule 1 - Bug] cw-storage-plus 1.x incompatible with cosmwasm-std 2.x**
- **Found during:** Task 1 (compilation errors after switching to 2.x)
- **Issue:** KeyDeserialize trait requires `KEY_ELEMS: u16` constant in 2.x; cw-* ecosystem crates used 1.x API incompatible with 2.x cosmwasm-std types
- **Fix:** Upgraded cw-storage-plus, cw-utils, cw-multi-test, cw2, cw20, cw20-base to 2.0.0; uncommented KEY_ELEMS in layer-std
- **Files modified:** Cargo.toml, packages/std/src/account_id.rs
- **Verification:** Storage plus tests pass (65 tests)
- **Committed in:** 6b36802 (Task 1)

**4. [Rule 3 - Blocking] cw-orch-core v1 to v2 API incompatibility in packages/golem**
- **Found during:** Task 1 (30+ compilation errors in golem)
- **Issue:** cw-orch-core 2.x changed trait signatures; 30+ errors in packages/golem
- **Fix:** Excluded packages/golem from workspace members; deferred migration
- **Files modified:** Cargo.toml
- **Verification:** Workspace builds without golem
- **Committed in:** 6b36802 (Task 1)

**5. [Rule 1 - Bug] Multiple cosmwasm-std 2.x API breakages**
- **Found during:** Task 2 (v1 to v2 upgrade)
- **Issue:** ContractInfoResponse::default() removed; CodeInfoResponse::new() signature changed; SubMsg missing payload; Reply missing gas_used/payload; SubMsgResponse missing msg_responses; InstanceOptions removed print_debug field; Checksum moved from cosmwasm_vm to cosmwasm_std; OverflowError::new() takes 1 arg not 3
- **Fix:** Updated all call sites in keeper.rs, backend.rs, cache.rs, utils.rs, query.rs, item.rs, caller contract
- **Files modified:** packages/app/src/wasm/vm/backend.rs, cache.rs, utils.rs, keeper.rs; packages/std/src/query.rs; packages/storage/src/plus/item.rs; contracts/caller/src/contract.rs
- **Verification:** All 112 tests pass; workspace builds clean
- **Committed in:** 4bb5ad2 (Task 2)

---

**Total deviations:** 5 auto-fixed (1 Rule 1 - URL 404 workaround, 2 Rule 3 - blocking build issues, 1 Rule 1 - ecosystem API breakage, 1 Rule 3 - golem deferred)
**Impact on plan:** All auto-fixes necessary for correctness. Golem package exclusion is a known deferral, not scope creep.

## Issues Encountered

- The patch.crates-io mechanism in Cargo requires both the version pinned in `[workspace.dependencies]` AND the `version = "2.3.2"` in the submodule package Cargo.toml to match exactly. Any mismatch causes "patch not used" warnings and the wrong version being resolved. Solved by aligning both.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- CosmWasm v2.3.2 foundation is solid; all workspace crates compile against it
- BackendApi trait is extensible with 14 Ethereum stubs ready for Phase 4 override
- packages/golem needs cw-orch-core v2 migration before it can re-enter workspace
- The lay3r-v2.3.2 branch in lib/cosmwasm is the integration point for future Ethereum runtime changes

## Self-Check: PASSED

- lib/cosmwasm/packages/vm/src/backend.rs: FOUND
- packages/app/src/wasm/vm/backend.rs: FOUND
- packages/app/src/wasm/vm/cache.rs: FOUND
- Commit 6b36802 (Task 1): FOUND
- Commit 4bb5ad2 (Task 2): FOUND
- Commit 6e24baf (Task 3): FOUND

---
*Phase: 01-foundation*
*Completed: 2026-03-18*
