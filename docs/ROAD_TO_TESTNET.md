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

## Known gaps and pre-existing failures

- **`layer-cosmos` fixture test failures — pre-existing.** `cargo test
  --workspace` fails in the layer-cosmos fixtures; `slay3rd` is green. Not a
  regression of this week's work, but they must either be fixed or pinned +
  documented before CI gating means anything.
- ~~Catch-up is lazy, not a sync protocol.~~ **Bulk backfill landed
  2026-09-29** (unverified live): `FetchRequest::HeightRange` P2P msg,
  `PayloadStore` height→digest index, `backfill_tick` every 250 ms requests
  all missing heights tip+1..observed in ≤64-height ranges; solicited-height
  replies bypass the lookahead window; retention raised 1,024→65,536 heights.
  Unit-tested (`test_backfill_tick_requests_missing_range` et al.).
  **Remaining gap:** (a) live verification — stop a node for ~10k blocks,
  measure time-to-tip vs the old digest crawl; (b) **state-sync** — a new
  validator still must execute every block from genesis; backfill only
  parallelises payload *acquisition*. Snapshot-based state-sync remains the
  missing piece for fast validator joins.
- **Catch-up side-effect observed:** while behind, a node logs
  `verify: rejecting proposal` / `certify: payload unavailable within wait
  window` — correct behaviour (it can't vote for payloads it doesn't hold)
  but it means a lagging validator contributes nothing until caught up.
- **`Simulate` = `Unimplemented`.** `GetTx` now works; `Simulate` is the
  remaining query-service gap — required for any external integrator.
- **Hybrid PQ tx auth landed 2026-09-29** (chain-side): `PubKey::Hybrid
  Secp256k1Mayo` — account spends require BOTH secp256k1 AND MAYO-1/2/3/5
  sigs over the same sign-doc hash; `Any` type_url
  `/junoclaw.crypto.HybridSecp256k1Mayo` parses into auth; +25k gas for the
  PQ half; domain-separated address space. Vendored `junoclaw-mayo-verify`
  into `packages/` for native use. **Remaining:** tx-sender signing support
  (MAYO signer = sriracha/C → Linux/Docker only), live hybrid tx on devnet,
  then validator identity + consensus cert hybridization (Phase 2 design).
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
3. ~~Bulk catch-up~~ **Verify backfill live + ship state-sync.** Test: stop
   node for 10k blocks, measure time-to-tip; new node from genesis or
   snapshot. Backfill code landed 2026-09-29 — needs a node-image rebuild
   and the live fault test to close.

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
