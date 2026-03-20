---
gsd_state_version: 1.0
milestone: v2.3
milestone_name: milestone
status: completed
stopped_at: Completed 02.1-03-PLAN.md — consensus bug fix, cross-process relay, testnet reaches block 10 with identical app_hash
last_updated: "2026-03-20T16:04:16.696Z"
last_activity: 2026-03-20 — Plan 02.1-01 complete; NodeConfig with PeerConfig/data_dir, LayerGrpcService with BroadcastTx and SyncQuery impls
progress:
  total_phases: 7
  completed_phases: 3
  total_plans: 11
  completed_plans: 11
  percent: 80
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-18)

**Core value:** WAVS programs can read from and write to persistent Layer state — enabling Layer to serve as the stateful backbone of the EigenLayer meta-chain ecosystem, with state anchored to Ethereum via zkVM proofs.
**Current focus:** Phase 1 — Foundation

## Current Position

Phase: 2.1 of 7 (Functional Node) — IN PROGRESS
Plan: 1 of 3 in current phase — COMPLETE
Status: Phase 02.1 plan 01 complete; NodeConfig updated, grpc.rs created with BroadcastTx + state sync
Last activity: 2026-03-20 — Plan 02.1-01 complete; NodeConfig with PeerConfig/data_dir, LayerGrpcService with BroadcastTx and SyncQuery impls

Progress: [████████--] 80% (Phase 2 complete; Phase 2.1 plan 1/3 done)

## Performance Metrics

**Velocity:**
- Total plans completed: 3
- Average duration: ~93 min
- Total execution time: ~4.8 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 01-foundation | 3/3 | ~4.8h | ~96 min |

**Recent Trend:**
- Last 5 plans: 01-01 (workspace cleanup), 01-02 (CosmWasm v2 upgrade), 01-03 (unsafe transmute elimination)
- Trend: On track

*Updated after each plan completion*
| Phase 01-foundation P01 | 6 | 2 tasks | 32 files |
| Phase 01-foundation P02 | 180 min | 3 tasks | 13 files |
| Phase 01-foundation P03 | 50 | 2 tasks | 4 files |
| Phase 02-commonware-consensus P01 | 6 | 2 tasks | 6 files |
| Phase 02-commonware-consensus P02 | 20 | 2 tasks | 5 files |
| Phase 02-commonware-consensus P03 | 90 | 2 tasks | 12 files |
| Phase 02-commonware-consensus P04 | 10 | 2 tasks | 6 files |
| Phase 02-commonware-consensus P05 | 4 | 1 tasks | 3 files |
| Phase 02.1-functional-node P01 | 45 | 2 tasks | 5 files |
| Phase 02.1-functional-node P02 | 13 | 2 tasks | 4 files |
| Phase 02.1-functional-node P03 | 144 | 2 tasks | 5 files |

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
- [Phase 01-foundation]: transmute on raw pointers retained for fat pointer lifetime erasure in make_backend — *ptr not &ref, meaningfully safer than original reference transmute
- [Phase 01-foundation]: miri blocked by wasmer FFI and file I/O syscalls — documented as expected limitation, functional correctness verified by regular test suite
- [Phase 02-commonware-consensus]: Commonware 2026.3.0 pinned exactly (ALPHA software; version confirmed via crates.io API 2026-03-19)
- [Phase 02-commonware-consensus]: HashSet retained in capabilities() at cosmwasm_vm CacheOptions boundary; BTreeSet cannot satisfy impl Into<HashSet<String>>; capabilities() not in certify/verify paths
- [Phase 02-commonware-consensus]: BTreeMap replaces HashMap in VmStore.iterators (backend.rs) and ValidCoins.seen (bank/keeper.rs) for determinism in App finalize_block paths
- [Phase 02-commonware-consensus]: LayerNode generic over P: PublicKey (not hardcoded to BLS12-381) — allows unit tests with ed25519::PublicKey and future scheme swaps without changing the bridge
- [Phase 02-commonware-consensus]: Tx deserialization deferred to Plan 04 — layer_std::Tx has no serde::Deserialize impl; Phase 2 certify passes empty Tx slice; correct wire format confirmed in Plan 04 integration testing
- [Phase 02-commonware-consensus]: Sync #[test] + block_on(new_current_thread) pattern for App<T> tests — wasmer JIT mmap initialization not safe for concurrent OS threads; eliminates SIGBUS on macOS
- [Phase 02-commonware-consensus]: Relay trait in actual commonware 2026.3.0 has only type Digest (no Plan/PublicKey); broadcast() takes only digest; research doc had stale interface
- [Phase 02-commonware-consensus]: Phase 2 relay is in-process only (shared pending_payloads Arc); no actual P2P; Phase 3 TODO for authenticated channels
- [Phase 02-commonware-consensus]: BLS sharing reconstructed from ChaCha8Rng::seed_from_u64(0) matching keygen tool; Phase 3 TODO: serialize Sharing to JSON
- [Phase 02-commonware-consensus]: commonware_runtime::Metrics must be imported for .with_label() on tokio::Context; use commonware_p2p::Manager as _; for trait method visibility
- [Plan 02-04]: verify-cert uses G1::decode + ops::verify_message::<MinSig> directly (avoids protocol-specific Subject/Namespace types in standalone tool)
- [Plan 02-04]: testnet.sh documents Phase 3+ launch procedure; Phase 2 in-process simulated P2P cannot span separate OS processes — cross-process consensus requires Phase 3 authenticated channels
- [Phase 02-CONFIRMED]: commonware_p2p::simulated is designed for commonware_runtime::deterministic only; when paired with commonware_runtime::tokio it calls TcpListener::bind(OsRng.next_u32() as random IPv4) which fails with BindFailed on macOS — Phase 3 MUST switch to commonware_p2p::authenticated
- [Plan 02-04]: verify-consensus.sh supports SKIP_CRASH_TEST=1 for environments without process isolation
- [Phase 02-commonware-consensus]: Certificate stored under '_cert/{height}' key with '_' prefix — excluded from app_hash, same as LAST_BLOCK, because certificate delivery timing is asynchronous and must not affect consensus determinism
- [Phase 02-commonware-consensus]: LayerReporter changed from stateless unit struct to stateful struct holding Arc<Mutex<App<T>>> for post-commit certificate persistence (CONS-05)
- [Plan 02.1-01]: NodeConfig.data_dir replaces both rocksdb_path and wal_path — single field, computed subpaths via wal_path() and app_data_path() methods; PeerConfig struct added for [[peers]] TOML table
- [Plan 02.1-01]: SyncProvider trait imported as layer_app::SyncProvider — the sync module is private; trait is re-exported from layer_app crate root
- [Plan 02.1-01]: LayerGrpcService<T> generic over PersistentStorage — same pattern as LayerNode; enables MemoryStore unit tests and RocksDB production use
- [Phase 02.1-functional-node]: lookup Oracle implements Blocker directly — no oracle.control(pk) needed for simplex config
- [Phase 02.1-functional-node]: Manual Clone impl required for LayerReporter<T> and LayerGrpcService<T> — Arc wrapping means T: Clone not needed; RockStore doesn't impl Clone
- [Phase 02.1-functional-node]: Cosmos query HTTP/2 tower layer deferred to Plan 03 — Plan 02 wires handle_cosmos_query import; tower/axum deps available as transitive but not direct
- [Phase 02.1-functional-node]: BLS share selection uses sorted Ed25519 key position not validator_index — keygen assigns shares[i] to the validator whose sorted key position is i; participant_keys must be sorted before ordered::Set construction
- [Phase 02.1-functional-node]: Cross-process payload relay: LayerRelay holds mpsc sender; background tasks send payload bytes via P2P channel 3 (Recipients::All) to peers; receivers insert into pending_payloads for verify()
- [Phase 02.1-functional-node]: Block timestamp = GENESIS_TIME_NS + view * 1e9 ns — deterministic, monotonic, always > genesis time; raw view * 1e9 produced pre-genesis timestamps failing DescendingBlockTime check

### Roadmap Evolution

- Phase 2.1 inserted after Phase 2: Functional Node (URGENT) — get real cross-process P2P, working CosmWasm contract deployment/execution, and a functional gRPC interface before starting the Ethereum type migrations

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 3]: Bech32 → H160 RocksDB key migration must be atomic with a `WriteBatch`; silent data corruption is the highest-risk failure in the project
- [Phase 2]: Commonware is ALPHA software (2026.3.0); API may change; mempool and block format are entirely application-defined with no framework guidance
- [Phase 6]: wreth architecture is undocumented publicly; must be clarified internally before Phase 6 planning can proceed

## Session Continuity

Last session: 2026-03-20T16:04:16.694Z
Stopped at: Completed 02.1-03-PLAN.md — consensus bug fix, cross-process relay, testnet reaches block 10 with identical app_hash
Resume file: None
