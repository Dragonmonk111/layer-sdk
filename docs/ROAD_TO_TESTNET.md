# Road to Testnet — Deterministic Plan

As of 2026-09-28, post re-genesis. Every item lists what is *verifiably true
today* vs. what remains, with the check that proves it done.

## Verified baseline (devnet, measured)

| Property | Evidence | Date |
|---|---|---|
| 4-validator Simplex, ~10 blk/s finality | live devnet, BLS certs every block | 2026-09-28 |
| Atomic `state_root`+payload commit | `finalize_block_with_payload`, single store tx | A1 |
| Decoupled execution / consensus voter | A2 |
| Mempool v2 (dedupe, peek, remove-on-commit, recheck) | A3; live: stale & gapped sequences rejected at `check_tx` (`InvalidSequence`) | 2026-09-28 |
| Bounded peer-pushed payloads | A4 + `test_accept_peer_payload_bounds` | 2026-09-28 |
| Single-node restart resume | node-3 restart → finalizing in <15 s | B5 |
| 60 s outage catch-up | node-2 −800 blk → caught up via peer payload relay | B5 |
| Full-network restart | all 4 resumed from persisted state, finalizing | B5 |
| `GetTx` implemented | `tx-sender get-tx` → height/code/gas/events | 2026-09-28 |
| Deterministic genesis | re-genesis reproduces `code_id 1` `layer_root.wasm`, same checksum/creator/address — verified against backup | 2026-09-28 |
| IBC ↔ local Osmosis | scripted rebuild ~1.5 min (`ibc-rebuild.ps1`), ICS-20 voucher minted | 2026-09-28 |
| Relay daemon | `relay` subcommand, catch_unwind + backoff + health `:18080` | 2026-09-28 |
| MAYO-2/3/5 verify in-contract | gas 309k/400k/726k vs 4M limit; tampered sig rejected | 2026-09-27 (pre-regenesis heights) |
| Hybrid secp256k1+MAYO account spend | committed h165,266 + h169,044; corrupted MAYO sig rejected at check_tx | 2026-09-29 |
| Bulk backfill | node-2 −700 blk → ~1,600 heights in ~15 min, ~1.7× live rate, identical `app_hash` | 2026-09-29 |
| Snapshot state-sync | wiped node-3 adopted cert-anchored snapshot @212,930, proposed @213,035 | 2026-09-30 |
| `Simulate` gRPC | full `execute_tx` on scratch overlay, real gas/error log, zero writes | 2026-09-30 |
| Hybrid consensus scheme (Phase 2a) | `hybrid_scheme.rs` — BLS threshold + MAYO2 bitmap cert, flag-gated; check+tests green, **live verify pending** | 2026-10-01 |

## Known gaps and pre-existing failures

- **`layer-cosmos` fixture test failures — pre-existing.** `cargo test
  --workspace` fails in the layer-cosmos fixtures; `slay3rd` is green. Not a
  regression of this week's work, but they must either be fixed or pinned +
  documented before CI gating means anything.
- ~~Catch-up is lazy, not a sync protocol.~~ **Bulk backfill live-verified
  2026-09-29** (node-2 −700 blk → caught up at ~1.7× rate). **Snapshot
  state-sync live-verified 2026-09-30** — cert-anchored chunked dump,
  verify-before-write, atomic commit; wiped node-3 adopted @212,930 and
  proposed @213,035. **Remaining:** the joiner anchored on one donor;
  multi-peer quorum (`state_sync.peers` + `min_anchor_agree`) is
  implemented 2026-10-01, live verify pending.
- **Catch-up side-effect observed:** while behind, a node logs
  `verify: rejecting proposal` / `certify: payload unavailable within wait
  window` — correct behaviour (it can't vote for payloads it doesn't hold)
  but it means a lagging validator contributes nothing until caught up.
- ~~**`Simulate` = `Unimplemented`.~~ **Done 2026-09-30** — `App::simulate`
  runs the full `execute_tx` path metered on a scratch overlay; gRPC handler
  maps GasInfo/events into `SimulateResponse`. Integrators can dry-run.
- **Hybrid PQ tx auth live-verified 2026-09-29**: `PubKey::Hybrid
  Secp256k1Mayo` spends committed @165,266 and @169,044 (`code=0`,
  ~110k gas); corrupted-MAYO-sig rejected at check_tx. Signing via
  `tools/hybrid-sign` Docker image + `tx-sender broadcast`.
  **Remaining:** validator identity + consensus cert hybridization —
  Phase 2a implemented 2026-10-01 (`hybrid_scheme.rs` + `hybrid_consensus`
  flag + MAYO keygen in the keygen tool), live devnet verify pending.
- **Mempool queues nothing.** Strict sequence at admission (stale *and*
  future nonces rejected). Simple and safe; means bursts must arrive
  ordered. Note for the faucet/bot docs.
- **Keys are devnet-grade.** Validator `keys.json` were committed publicly
  and are seed-0 derivable (now untracked + backed up; see
  `KEY_MANAGEMENT.md`). Relayer key lives only in a container keyring
  (archived to `backups/local-osmosis-keyring-2026-09-28`). One funded key
  (`osmo1aq995`) was already lost to process failure.
- **Wasm gas metering unhardened.** Honest MAYO-5 = 726k/4M; adversarial
  worst-case unmeasured. Per-tx gas cap is the only stall protection.
- **No adversarial testing of the PQ stack.** KAT vectors pass; no fuzzing
  of `junoclaw-mayo-verify`, pk-hash path, or Bud weight arithmetic.
- **No external review of the MAYO port.**

## Ordered plan

### Phase 0 — close known debt (prereq for everything)
1. Fix or pin `layer-cosmos` fixture failures; make `cargo test --workspace`
   a green gate.
2. `Simulate` in the query service (follows `GetTx` pattern).
3. ~~Bulk catch-up + state-sync~~ **Both live-verified** (Sept 29/30).
   Remaining: live-verify the multi-peer anchor quorum (implemented
   2026-10-01) — fresh node with `peers=[2 donors]`, `min_anchor_agree=2`,
   plus a mismatched-donor refusal test.

### Phase 1 — hardening
4. Fuzz the PQ stack + malformed wasm executes; measure adversarial gas.
5. Per-tx gas cap / metering audit.
6. Process supervision for relayer + nodes (compose restart policies are in
   place; production = service manager + health checks wired to alerts).
7. Transaction/restart chaos pass: repeat B4+B5 while `tx-sender` load runs.

### Phase 2 — ceremony
8. Key ceremony: DKG path (`OsRng`, per-operator shares), real deployer key,
   keyring relayer keys with backed-up mnemonics — per `KEY_MANAGEMENT.md`.
9. Genesis ceremony dry-run by someone who didn't write
   `VALIDATOR_ONBOARDING.md`; faucet for `ujclaw`.
10. Re-verify deterministic genesis against a checksum-pinned input set
    (same procedure used post-wipe: `jc-codes`/`jc-contracts` diff).

### Phase 3 — visibility
11. Minimal status page: height, peers, last finality, code/contract list.
12. External MAYO port review; broaden to audit once surface settles.

### Testnet gate
Everything in Phase 0 + Phase 1 items 4–5 green, plus one full-dress
rehearsal: ceremony → genesis → IBC link → transfer → contract deploy →
PQ verify, scripted end-to-end like `ibc-rebuild.ps1` did for the bridge.
