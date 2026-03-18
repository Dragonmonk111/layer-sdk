---
phase: 01-foundation
plan: 03
subsystem: wasm
tags: [cosmwasm, unsafe-rust, raw-pointers, determinism, sha256]

# Dependency graph
requires:
  - phase: 01-02
    provides: "CosmWasm v2.3.2 integration with updated Backend types"
provides:
  - "make_backend: raw pointer backend construction replacing danger_will_robinson transmute"
  - "Documented safety invariants on VmStore and VmQuerier"
  - "counter_address_is_deterministic regression test proving FOUND-03"
affects: [02-consensus, 04-wasm-runtime]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Raw pointer fat pointer lifetime erasure via transmute(*ptr) in unsafe fn — replaces reference transmute"
    - "unsafe impl Send for structs holding raw pointers with single-threaded scope guarantee"
    - "SAFETY comment at every unsafe dereference site"

key-files:
  created: []
  modified:
    - packages/app/src/wasm/vm/backend.rs
    - packages/app/src/wasm/vm/cache.rs
    - packages/app/src/wasm/utils.rs
    - packages/app/src/testing/utils.rs

key-decisions:
  - "transmute on raw pointers retained for fat pointer lifetime erasure (*mut dyn Storage not reference — significantly safer than original transmute::<&mut, &mut>)"
  - "miri blocked by wasmer FFI and file I/O syscalls — documented as expected limitation, functional tests confirm correctness"
  - "hex::encode called via full path (not `use hex`) to avoid conflict with hex_literal::hex macro name"

patterns-established:
  - "Raw pointer pattern: cast &mut dyn Trait to *mut (dyn Trait + 'static) via transmute in unsafe fn body, dereference at each access site with SAFETY comment"
  - "All unsafe access sites get one-line SAFETY: pointer valid for cache call duration (see make_backend docs)"

requirements-completed: [FOUND-02, FOUND-03]

# Metrics
duration: 50min
completed: 2026-03-18
---

# Phase 1 Plan 3: Unsafe Transmute Elimination and Address Determinism Proof Summary

**danger_will_robinson reference transmute replaced with make_backend raw pointer construction; v1 counter-based address determinism proven by SHA256 regression test**

## Performance

- **Duration:** 50 min
- **Started:** 2026-03-18T16:48:52Z
- **Completed:** 2026-03-18T17:39:09Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Eliminated `danger_will_robinson` — the unsafe function using `transmute` on references to extend lifetimes — replaced with `make_backend` using raw pointers with documented invariants
- VmStore and VmQuerier now hold `*mut`/`*const` pointers instead of `&'static` references; `unsafe impl Send` added with documented single-threaded scope guarantee
- All 6 call sites in cache.rs (instantiate, execute, migrate, sudo, reply, query) updated to use `make_backend` with SAFETY comments
- Added `counter_address_is_deterministic` test proving `build_instantiate_address` is pure SHA256 with no SystemTime or rand — satisfying FOUND-03
- Fixed pre-existing test utility compile error (`k256::Signature::to_vec()` removed in newer k256; changed to `to_bytes().to_vec()`)

## Task Commits

Each task was committed atomically:

1. **Task 1: Replace transmute with raw pointer backend construction** - `add79ca` (feat)
2. **Task 2: Add determinism regression test for v1 contract address generation** - `b8b6156` (test)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified

- `packages/app/src/wasm/vm/backend.rs` - Replaced `danger_will_robinson` with `make_backend`; VmStore/VmQuerier use raw pointers; all method bodies use unsafe dereferences with SAFETY comments
- `packages/app/src/wasm/vm/cache.rs` - Updated import and all 6 call sites to use `make_backend`
- `packages/app/src/wasm/utils.rs` - Added `counter_address_is_deterministic` regression test
- `packages/app/src/testing/utils.rs` - Fixed pre-existing k256 `to_vec()` → `to_bytes().to_vec()` compile error

## Decisions Made

- **transmute retained for fat pointer lifetime erasure:** Rust's compiler rejects direct `*mut dyn Storage` assignment from shorter-lived references because `dyn Storage` in struct fields defaults to `dyn Storage + 'static`. The solution is `transmute(ref as *mut dyn Storage)` which operates on the raw pointer type (not on a reference). This is meaningfully safer than the original which used `transmute::<&mut dyn Storage, &mut dyn Storage>` (extending a reference's lifetime). The unsafe contract is unchanged — the safety invariant is documented in `make_backend`'s doc comment.

- **miri limitation documented:** `cargo +nightly miri test` fails on `wasm::vm` tests because `remove_dir_all` (file I/O in test setup) is an unsupported syscall under miri. Wasmer FFI would also prevent miri from instrumenting the WASM execution path. This is expected per the plan's note. Correctness is verified by the regular test suite (3 cache tests passing).

- **hex::encode via full path:** The `hex` crate exports a macro also named `hex` which conflicts with `hex_literal::hex`. Used `hex::encode(...)` directly without a `use hex;` import to avoid the name collision.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed pre-existing k256 Signature::to_vec() compile error in testing utils**
- **Found during:** Task 1 verification (running cache tests)
- **Issue:** `packages/app/src/testing/utils.rs` line 342 called `.to_vec()` on `k256::ecdsa::Signature`, which was removed in a newer k256 version. This blocked all lib test compilation.
- **Fix:** Changed `signature.to_vec().into()` to `signature.to_bytes().to_vec().into()`
- **Files modified:** `packages/app/src/testing/utils.rs`
- **Verification:** `cargo test -p layer-app --lib -- wasm::vm::cache` exits 0 (3 tests pass)
- **Committed in:** `add79ca` (Task 1 commit)

**2. [Rule 1 - Bug] transmute on raw pointers required for fat pointer lifetime erasure**
- **Found during:** Task 1 (initial compilation)
- **Issue:** Rust compiler rejects `*mut dyn Storage` struct fields when initialized from shorter-lived references (`'1 must outlive 'static`). The plan specified removing `mem::transmute` entirely, but the fat pointer vtable lifetime constraint requires it on the pointer type.
- **Fix:** Retained `mem::transmute` only for the specific cast `*mut dyn Storage → *mut (dyn Storage + 'static)` and `*const dyn ReadonlyStorage → *const (dyn ReadonlyStorage + 'static)`. This is strictly a pointer-level lifetime annotation erasure, not a reference transmute. The original unsafe contract is preserved and documented.
- **Files modified:** `packages/app/src/wasm/vm/backend.rs`
- **Verification:** `cargo build --workspace` exits 0; no `transmute` on references remains
- **Committed in:** `add79ca` (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 Rule 1 bugs)
**Impact on plan:** Both auto-fixes necessary for compilation and correctness. No scope creep. The spirit of FOUND-02 is fully met — reference transmute is eliminated; only pointer-level transmute for fat pointer lifetime annotation remains.

## Issues Encountered

- Rust's variance rules for fat pointers (`*mut dyn Trait`) require `dyn Trait + 'static` in struct fields, which rejected direct assignment from shorter-lived references even within an `unsafe fn`. Required using `transmute` at the pointer level to erase the lifetime annotation. The plan's intent (eliminating dangerous reference lifetime extension) is fully achieved.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- FOUND-02 satisfied: `danger_will_robinson` reference transmute eliminated; Phase 2 (Commonware async contexts) can safely add async execution without the reference lifetime lie
- FOUND-03 satisfied: `counter_address_is_deterministic` regression test anchors address determinism
- Phase 1 (Foundation) all 3 plans complete — ready for Phase 2 (Consensus)

## Self-Check: PASSED

- `packages/app/src/wasm/vm/backend.rs` — FOUND
- `packages/app/src/wasm/vm/cache.rs` — FOUND
- `packages/app/src/wasm/utils.rs` — FOUND
- `.planning/phases/01-foundation/01-03-SUMMARY.md` — FOUND
- Commit `add79ca` (Task 1: make_backend) — FOUND
- Commit `b8b6156` (Task 2: determinism test) — FOUND
- Commit `939720f` (docs: plan complete) — FOUND
- No `danger_will_robinson` in backend.rs or cache.rs — VERIFIED
- No reference transmutes remaining — VERIFIED
- 6 `make_backend(` calls in cache.rs — VERIFIED
- `counter_address_is_deterministic` test present — VERIFIED

---
*Phase: 01-foundation*
*Completed: 2026-03-18*
