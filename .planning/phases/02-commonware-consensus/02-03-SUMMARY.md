---
phase: 02-commonware-consensus
plan: "03"
subsystem: consensus
tags: [commonware, simplex, bls12381, dkg, p2p, relay, consensus-runtime, tokio-runner]

# Dependency graph
requires:
  - phase: 02-commonware-consensus
    provides: "LayerNode CertifiableAutomaton (02-02), BlockPayload, Mempool, determinism audit (02-01)"
  - phase: 01-foundation
    provides: "CosmWasm v2 upgrade, safe App<T> without unsafe transmute"
provides:
  - "NodeConfig struct loading from TOML with all consensus parameters"
  - "Block.certificate field for BLS threshold signature storage (CONS-05 groundwork)"
  - "Offline BLS DKG keygen tool for 3-node testnet (tools/generate-testnet-keys)"
  - "LayerRelay implementing Relay trait with shared pending_payloads Arc (non-proposer verify)"
  - "main.rs: full consensus node entry point using commonware_runtime::tokio::Runner"
  - "slay3rd binary compiles and starts (exits cleanly if key material is missing)"
affects:
  - 02-commonware-consensus
  - 03-ethereum-types
  - 05-wavs

# Tech tracking
tech-stack:
  added:
    - "commonware-p2p::simulated (Oracle, Network, Control, Link) for in-process P2P"
    - "commonware-runtime::Metrics trait (required for with_label() on Context)"
    - "commonware-parallel::Sequential strategy"
    - "commonware_utils::{N3f1, NZU16, NZUsize, ordered::Set}"
    - "toml = 0.8 for NodeConfig TOML loading"
    - "rand + rand_chacha for DKG seeded RNG"
    - "deal_anonymous::<MinSig, N3f1> for offline threshold DKG"
  patterns:
    - "Relay trait: type Digest only (no Plan or PublicKey — actual API differs from research)"
    - "RoundRobin<Sha256>::default() needs explicit type param to avoid inference ambiguity"
    - "use commonware_runtime::Metrics; required to call .with_label() on tokio::Context"
    - "Chain oracle.control(pk).register(channel, quota) to avoid E type inference failure"
    - "use commonware_p2p::Manager as _; for mgr.track() trait method resolution"
    - "Runner::start() closure pattern for consensus engine (not #[tokio::main])"
    - "LayerRelay shares pending_payloads Arc with LayerNode for non-proposer verify()"

key-files:
  created:
    - "app/slay3rd/src/config.rs — NodeConfig struct with TOML loading"
    - "app/slay3rd/src/relay.rs — LayerRelay implementing Relay trait"
    - "tools/generate-testnet-keys/Cargo.toml — standalone DKG keygen crate"
    - "tools/generate-testnet-keys/src/main.rs — offline BLS DKG for 3-node testnet"
  modified:
    - "packages/std/src/api/block.rs — added certificate: Option<Vec<u8>> field"
    - "packages/app/src/testing/utils.rs — certificate: None in Block construction"
    - "packages/app/src/app.rs — certificate: None in 2 Block construction sites"
    - "app/slay3rd/src/node.rs — certificate: None in execute_block()"
    - "app/slay3rd/src/main.rs — full consensus node entry point (rewritten)"
    - "app/slay3rd/src/lib.rs — added pub mod config and pub mod relay"
    - "Cargo.toml — added toml, commonware-parallel, commonware-utils, commonware-codec, commonware-math workspace deps"
    - "app/slay3rd/Cargo.toml — added all new deps including cosmwasm-std, rand, rand_chacha"

key-decisions:
  - "Relay trait in actual commonware 2026.3.0 has only type Digest — no Plan or PublicKey associated types; broadcast() takes only digest. Research doc had incorrect/stale interface."
  - "Phase 2 relay is in-process only (no actual P2P): broadcast() serializes to payload_store; pending_payloads sharing is the synchronization mechanism for localhost testing"
  - "BLS sharing reconstructed from same seeded RNG as keygen tool (ChaCha8Rng::seed_from_u64(0)) — pragmatic Phase 2 workaround; Phase 3 TODO: serialize Sharing to JSON"
  - "RoundRobin<Sha256> explicit type annotation required — default type param inference fails when SimplexConfig generic chain is long"
  - "commonware_runtime::Metrics trait must be in scope for with_label() — compiler error E0599 was misleading (method exists but trait not imported)"
  - "oracle.control(pk).register() must be chained — storing Control<P,E> in variable creates type inference failure for E (the context type)"

patterns-established:
  - "Always import commonware_runtime::Metrics when calling .with_label() on any context type"
  - "Chain oracle.control(pk).register() calls rather than binding control to a variable"
  - "Use use commonware_p2p::Manager as _; for trait method visibility on Manager"
  - "Relay sharing pattern: LayerRelay::new(layer_node.pending_payloads()) — same Arc enables non-proposer verify()"

requirements-completed: [CONS-01, CONS-03, CONS-05]

# Metrics
duration: ~90min (continuation session)
completed: 2026-03-19
---

# Phase 02 Plan 03: Consensus Runtime Wiring Summary

**BLS DKG keygen tool + NodeConfig + LayerRelay (shared pending_payloads) + full main.rs wiring commonware simplex Engine via tokio::Runner**

## Performance

- **Duration:** ~90 min (continuation from previous session)
- **Started:** ~2026-03-19T15:30:00Z (prior session)
- **Completed:** 2026-03-19T17:25:00Z
- **Tasks:** 2
- **Files modified:** 12 (created 4, modified 8)

## Accomplishments

- Full slay3rd binary compiles with commonware simplex consensus engine, BLS12-381 threshold signing, and in-process simulated P2P
- Offline BLS DKG keygen tool generates 3-validator key material using deal_anonymous with deterministic seed
- LayerRelay implements Relay trait with shared pending_payloads Arc enabling non-proposer verify()
- Block.certificate field added to packages/std Block struct for CONS-05 BLS certificate storage

## Task Commits

Each task was committed atomically:

1. **Task 1: BLS DKG keygen tool, NodeConfig, and Block.certificate field** - `7cdf076` (feat)
2. **Task 2: P2P Relay + main.rs consensus runtime wiring** - `4d53592` (feat)

**Plan metadata:** (this commit)

## Files Created/Modified

- `app/slay3rd/src/config.rs` - NodeConfig struct with TOML loading, all consensus parameters
- `app/slay3rd/src/relay.rs` - LayerRelay implementing Relay trait with shared pending_payloads Arc; receive_payload() for peer broadcasts; 4 unit tests
- `app/slay3rd/src/main.rs` - Full consensus node entry point: loads TOML config, BLS key material, creates App/LayerNode/LayerRelay, sets up simulated P2P, spawns simplex Engine via Runner::start()
- `app/slay3rd/src/lib.rs` - Added pub mod config and pub mod relay
- `packages/std/src/api/block.rs` - Added certificate: Option<Vec<u8>> field to Block struct
- `packages/app/src/testing/utils.rs` - certificate: None added to Block construction
- `packages/app/src/app.rs` - certificate: None added to 2 Block construction sites
- `app/slay3rd/src/node.rs` - certificate: None added to execute_block() Block construction
- `tools/generate-testnet-keys/Cargo.toml` - Standalone keygen crate (not workspace member)
- `tools/generate-testnet-keys/src/main.rs` - Offline BLS DKG: deal_anonymous::<MinSig, N3f1> with ChaCha8Rng seed, writes per-validator keys.json
- `Cargo.toml` - Added toml, commonware-parallel, commonware-utils, commonware-codec, commonware-math as workspace deps
- `app/slay3rd/Cargo.toml` - Added cosmwasm-std, rand, rand_chacha, serde_json and all new commonware deps

## Decisions Made

1. **Relay trait API correction**: The actual `commonware_consensus::Relay` trait has only `type Digest` — no `Plan` or `PublicKey` associated types. `broadcast()` takes only the digest. The research doc listed stale/incorrect interface. Adapted implementation accordingly.

2. **In-process relay for Phase 2**: broadcast() serializes payload to payload_store but does NOT send via P2P. The shared pending_payloads Arc handles synchronization for localhost testing. Phase 3 TODO: real P2P via authenticated channels.

3. **BLS DKG reconstruction via seeded RNG**: Phase 2 reconstructs the DKG sharing polynomial from the same ChaCha8Rng::seed_from_u64(0) used by the keygen tool (after skipping Ed25519 key generation). This avoids serializing the Sharing<V> polynomial. Phase 3 TODO: serialize Sharing directly.

4. **RoundRobin<Sha256> explicit type**: `RoundRobin::default()` fails type inference when used in a long generic chain. Using `RoundRobin::<commonware_cryptography::Sha256>::default()` is explicit and correct.

5. **Metrics trait pattern**: `commonware_runtime::Metrics` must be imported to call `.with_label()` on `tokio::Context`. Compiler error E0599 ("no method named with_label") is misleading — the method exists but the trait is not in scope.

6. **Chaining control.register()**: Storing `oracle.control(pk)` as a variable creates type inference failure for the `E` (context/clock) type parameter. Chaining `oracle.control(pk).register(ch, quota)` lets the compiler infer `E` from the oracle.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Relay trait API mismatch with research doc**
- **Found during:** Task 2 (LayerRelay implementation)
- **Issue:** Research doc stated `Relay` has `type Plan`, `type PublicKey`, and `broadcast(digest, plan)`. Actual API has only `type Digest` and `broadcast(digest)`.
- **Fix:** Adapted LayerRelay to implement the actual API. Removed non-existent associated types. Simplified broadcast() signature.
- **Files modified:** app/slay3rd/src/relay.rs
- **Verification:** cargo build -p slay3rd exits 0
- **Committed in:** 4d53592 (Task 2 commit)

**2. [Rule 3 - Blocking] Missing workspace deps: commonware-utils, commonware-codec, commonware-math**
- **Found during:** Task 1 (keygen tool) and Task 2 (main.rs)
- **Issue:** These crates were used but not in workspace [workspace.dependencies]; also missing from slay3rd Cargo.toml
- **Fix:** Added all three to Cargo.toml workspace.dependencies and app/slay3rd/Cargo.toml
- **Files modified:** Cargo.toml, app/slay3rd/Cargo.toml
- **Committed in:** 7cdf076 (Task 1 commit) and 4d53592 (Task 2 commit)

**3. [Rule 1 - Bug] commonware_runtime::Metrics trait not imported for with_label()**
- **Found during:** Task 2 (main.rs compilation)
- **Issue:** E0599 "no method named with_label" on tokio::Context — compiler hint indicated Metrics trait not in scope
- **Fix:** Added `use commonware_runtime::Metrics;` to main.rs imports
- **Files modified:** app/slay3rd/src/main.rs
- **Committed in:** 4d53592 (Task 2 commit)

**4. [Rule 1 - Bug] Type inference failure for oracle.control().register()**
- **Found during:** Task 2 (main.rs compilation)
- **Issue:** E0282 "cannot infer type" when storing Control<P,E> as variable; E parameter ambiguous
- **Fix:** Chained oracle.control(pk).register(ch, quota) directly without intermediate binding
- **Files modified:** app/slay3rd/src/main.rs
- **Committed in:** 4d53592 (Task 2 commit)

**5. [Rule 1 - Bug] RoundRobin::default() type inference ambiguity**
- **Found during:** Task 2 (main.rs compilation)
- **Issue:** E0283 "type annotations needed for Config" — RoundRobin<H: Hasher> needs explicit H type when used in complex generic Config chain
- **Fix:** Changed to `RoundRobin::<commonware_cryptography::Sha256>::default()`
- **Files modified:** app/slay3rd/src/main.rs
- **Committed in:** 4d53592 (Task 2 commit)

---

**Total deviations:** 5 auto-fixed (1 incorrect API, 2 blocking, 2 type inference bugs)
**Impact on plan:** All auto-fixes were blocking compilation. No scope creep. All fixes are correctness requirements.

## Issues Encountered

1. **API documentation mismatch**: Research doc had the `Relay` trait with extra associated types. Discovered by reading generated docs from the installed crate. This is a known risk with ALPHA software (Commonware 2026.3.0). Pattern: always read actual docs, not research summaries, for ALPHA crates.

2. **Multiple Cargo.toml additions**: The keygen tool and main.rs required 8 dependency additions across workspace and package Cargo.toml files. Accumulated debt from the research phase not confirming all required transitive deps.

## User Setup Required

None — no external service configuration required. Key material generation uses the offline DKG tool.

## Next Phase Readiness

- slay3rd binary compiles and is runnable (requires key material from tools/generate-testnet-keys)
- Phase 2 full integration test (3-node consensus round) is ready to execute in Plan 04
- Block.certificate field is in place for CONS-05 certificate storage
- Phase 3 (Ethereum Types) can proceed independently of consensus testing

---
*Phase: 02-commonware-consensus*
*Completed: 2026-03-19*

## Self-Check: PASSED

- FOUND: app/slay3rd/src/config.rs
- FOUND: app/slay3rd/src/relay.rs
- FOUND: tools/generate-testnet-keys/src/main.rs
- FOUND: .planning/phases/02-commonware-consensus/02-03-SUMMARY.md
- FOUND: commit 7cdf076 (Task 1)
- FOUND: commit 4d53592 (Task 2)
