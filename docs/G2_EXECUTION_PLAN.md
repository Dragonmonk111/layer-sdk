# G2 Execution Plan — sequenced master plan

Last updated: 2026-10-12 (during T12 soak, pre-close-out).
Working rule: NOTHING touches the chain before close-out
(2026-10-13 ~19:58 UTC). Mixed-version P2P (tag-2 HeightRange +
hybrid certs are new-image-only) means every seat rebuilds
together in the rc10 window — never one at a time.

Call-prep companion docs (do not share 08):
`C:\Users\Taj\Downloads\div-call-prep\` — 01 reply (sent), 02 live
agenda, 03 reference-check notes, 04 airdrop decisions, 05 PR
review, 06 ops-group intro, 07 API surface (share-ready),
08 G1 close-check (internal only).

## Wave 0 — pre-close-out (now → 2026-10-13 19:58 UTC)

| # | task | status |
|---|---|---|
| W0.1 | Stall-stat reconciliation: `g1-ceremony/soak-stalls.mjs` (agreed freeze definition, coverage-gap-aware) + runbook T12 verdict rewording | DONE 2026-10-12 |
| W0.2 | Monitor watchdog (`monitor-watchdog.ps1` + task `JunoClawG1MonitorWatchdog`, 5-min). At close-out: disable task BEFORE stopping monitor | DONE 2026-10-12 |
| W0.3 | rc10 fix #1: `by_height` clobber — `PayloadStore::put` first-writer-wins, new `put_executed` (canonical override), execute path switched, 2 regression tests. **95/95 slay3rd lib tests green** | LANDED locally 2026-10-12 (deploy = Wave 2 R1) |
| W0.4 | rc10 fix #2 grounding: `P2pConfig::local()` is NOT a dial-count limit (diff vs `recommended()` = rate limits only; both share 5 s synchrony / 10 s handshake-age bounds). Evidence: ovh-uk1 journal shows live "timestamp too far ahead of the local clock" proposal rejections + partial mesh (payload relay reaches 2/3 peers). Root cause = clock skew, needs seat shells → Wave 2 | ANALYSIS DONE 2026-10-12 |
| W0.5 | Joiner rehearsal procedure | written (Wave 2 R6) |
| W0.6 | API surface doc v0.1 for InfraSingularity | DONE 2026-10-12 |
| W0.7 | 24 h checkpoint (2026-10-12 19:58 UTC): soak-stats + soak-stalls + `321`.md entry | scheduled |
| W0.8 | Pre-stage close-out — verdict draft (`g1-ceremony/CLOSE-OUT-VERDICT-DRAFT.md`), announcement draft (`junoclaw/drafts/DRAFT_ANNOUNCEMENT_G1_CLOSED_G2_OPEN_2026_10_13.md`), onboarding pack skeleton (`docs/G2_VALIDATOR_ONBOARDING.md` with [RC10]/[G2-KICKOFF] placeholders). Remaining: fill final numbers at close-out; site PR patches land with Ravi's users.md fix | DONE (drafts) 2026-10-12 |

## Wave 1 — close-out (2026-10-13 ~19:58 UTC)

Per `SOAK-CLOSEOUT-RUNBOOK.md` (amended 2026-10-12):
1. soak-stats + soak-stalls + g1-monitor --once
2. Disable `JunoClawG1MonitorWatchdog`, then stop the monitor
3. T12 verdict in the wedge/divergence framing — report freeze
   count/max from soak-stalls.mjs; NEVER write "zero stalls >90 s"
4. T13/T14 verdicts with logged residuals (home-vb host −83 s;
   akash unmeasurable; h312344 unverifiable artifact)
5. Append verdict block to `321`.md; update
   `docs/G1_SAFETY_AND_TEST_PLAN.md` results list
6. Declare G1 closed → public announcement with the 48 h numbers

## Wave 2 — rc10 window (chain may be touched; all seats together)

| # | task | notes |
|---|---|---|
| R1 | Build + deploy rc10 binary to ALL seats (ships the `by_height` clobber fix) | rebuild all at once |
| R2 | Clock fixes — likely THE L0/L1 remedy: coordinator `w32tm /resync` + fix time-service peers; audit VBox guest time-sync so a host jump can never teleport the guest clock; re-probe akash host clock | then re-verify skew < 2 s |
| R3 | akash stale lease: confirm current lease address with operator, redeploy, refresh all peer lists | literal IPs today |
| R4 | P2P WAN tuning IF R2 doesn't restore the full mesh: plumb `synchrony_bound` / `max_handshake_age` / `handshake_timeout` / dial knobs into node.toml | root-cause on seat shells first |
| R5 | Backfill through outbound-only NAT (akash serve-send): investigate the commonware lookup serve path | timeboxed; deeper work |
| R6 | JOINER REHEARSAL (G2 onboarding gate): 5th OBSERVER node state-syncs from 2 donors with `min_anchor_agree=2`; verify adoption + catch-up; then mismatched-donor refusal. Multi-peer anchor is devnet-verified (2026-10-01) but never exercised on this chain — and every G2 validator joins through it | procedure below |
| R7 | Re-measure block rate (expect ~2 s/blk) + confirm the 3 WARN classes are gone | |
| R8 | journald vacuum on ovh-uk1 (4.0G → ≤500M) + fresh VmRSS/VmHWM read for Ravi's T14 table | promised |
| R9 | DEVNET set-change rehearsal: full Phase-A ceremony on the devnet (add a validator, remove a validator, fresh threshold polynomial, coordinated halt+restart, verify the new seat proposes) — the practice run for the G2-opening transition | before any live set change |
| R10 | Coordinator-host hardening (protects home-vb + monitor + watchdog): WU — VERIFIED 2026-10-12: `wuauserv` Stopped/Manual, all pending-reboot flags False, no UpdateOrchestrator reboot task armed → nothing can fire; seatbelt policy NoAutoRebootWithLoggedOnUsers=1 APPLIED via `g1-ceremony/host-no-autoreboot.reg` — the box can no longer auto-reboot on updates even if wuauserv is ever re-armed. Sleep/hibernate timeouts → Never + Fast Startup off; VBox VM autostart (VBoxAutostart service) so a host reboot brings home-vb back; public-IP drift watch (scheduled compare vs pinned 84.9.189.220). Guest: restore ssh access (NAT keys broken) → audit guest chrony + runtime + archive the retired Juno-mainnet data (user-confirmed first). NOTE: Guest Additions NOT installed/reporting on this VM — confirmed no host→guest time-sync teleportation path | rc10 window |

### R6 procedure — joiner rehearsal
1. Build observer config: `node-observer.toml` — no validator keys,
   peers = 2 donors (ovh-uk1 + vahana literal IPs),
   `min_anchor_agree = 2`, chain_id `junoclaw-g1`.
2. Boot on spare capacity (ovh-uk1 has 60G free disk, 2 vCPU
   headroom, or a coordinator VBox VM). Observer has no keys — it
   cannot interfere with consensus.
3. Watch state-sync: ListSnapshots from both donors, anchor quorum
   agreement, adopt, catch-up to tip. Verify LatestHeight tracks
   and Block(h) serves records.
4. Negative leg: point a second joiner at one honest donor + one
   endpoint serving different bytes → must refuse (anchor
   disagreement, never adopt).
5. Log PASS in `321`.md as the G2-onboarding gate.

## Wave 3 — G2 kickoff (immediately after Wave 2 is green)

| # | task |
|---|---|
| K1 | Merge Ravi's site PR (with the users.md fix + G1-closed patches + final 48 h numbers) |
| K2 | Publish: validator onboarding doc, endpoints page, milestone roadmap |
| K3 | Public announcement (article/thread) |
| K4 | Send the onboarding pack: Anatoly (first ping) + Div's team |
| K5 | Faucet (P1): rate-limited test-ujclaw dispenser (contract + service; document the strict-sequence mempool for bot authors) |
| K6 | Status page (P1): height, peers, last finality, contracts, tx lookup — reuse g1-monitor/soak-stats; optional light-client "finality-verified" badge |
| K7 | Public endpoint hardening design (TLS, rate limits, tx-submission policy) — fold in InfraSingularity's edge offer after the call |
| K8 | Airdrop INTERNAL rehearsal (decision 2026-10-12): instantiate airdrop-claim with the authoritative root `d38cfb7a…`, exercise claim + double-claim rejection + sweep + the per-address proof API (313 MB merkle-proofs.json). Internal set only — the site's "no airdrop claim site" line stays until the mainnet claim |

## Wave 4 — early G2 hardening (P1, parallel with Wave 3)

| # | task |
|---|---|
| H1 | PQ-1: locate the Bud contract source → arithmetic fuzz (ML-DSA KATs may stay post-G2) |
| H2 | Adversarial gas measurement + per-tx gas cap audit (wasm metering unhardened; 4M cap is the only stall protection today) |
| H3 | Fix layer-cosmos fixtures → green `cargo test --workspace` CI gate |
| H4 | Document the CDC stream as the indexer interface (GetTxsEvent/GetBlockWithTxs stay unimplemented unless a partner needs them) |
| H5 | Relayer supervision: process manager + health-check-to-alerts |
| H6 | Alerting → ops contacts: seat alarm notifies the registered contact (the onboarding promise) |
| H7 | Promote the watchdog pattern to a real service for G2 monitors |

## Wave 5 — mainnet gate (P2; no dates until gates hold)

| # | task |
|---|---|
| M1 | Dealer-free DKG (coordinator currently deals BLS shares — documented trust tradeoff) |
| M2 | External MAYO port review → broaden to audit |
| M3 | ICS-20 channel over the 08-wasm client (BLS light client devnet-proven; uni-7/Osmosis testnet next) → ujclaw tradable |
| M4 | Governance handoff: multisig deploy → `gov_account` repoint → treasury transfer → ceremony key to cold storage (breakglass until G2+2 clean epochs) |
| M5 | Airdrop MAINNET event: fresh 90-day window, multisig-funded, two-stage article, official-domain-only claim page |
| M6 | Full-dress rehearsal: ceremony → genesis → IBC → transfer → contract deploy → PQ verify, scripted end-to-end |

## Decision log

- 2026-10-12 — Airdrop: internal rehearsal at G2; the real
  213k-recipient claim is a MAINNET event. (div-call-prep/04)
- 2026-10-12 — Site PR #1: merge after close-out with patches;
  formal G1 close stays at the documented 48 h gate. (…/05)
- 2026-10-12 — T12 verdict framing: wedge/divergence with the
  freeze count reported; "zero stalls >90 s" struck from the
  runbook. (runbook + `321`.md)
- 2026-10-12 — CDC stream is the indexer interface;
  GetTxsEvent/GetBlockWithTxs remain unimplemented by design.
  (…/07 API surface doc)
- 2026-10-12 — G2 set composition: **ovh-uk1 = primary builder
  seat** (datacenter, measured, NTP-clean, carries ~50% of
  proposals); home-vb = secondary OUR-side seat (after rc10 clock +
  peering fixes; documented single-host risk with the coordinator);
  **akash retired at the G2-opening set change** (log-only, stale
  lease, NAT-broken backfill serving, provider-readable keys).
  Set changes are ATOMIC: n must stay >= 3f+1 — never drop to 3.
  G2 target n=5 (f=1, tolerates 2 simultaneous crash-downs for
  liveness); mainnet target n=7 (f=2) per
  `junoclaw/drafts/VALIDATOR_SET_DKG_PLAN.md`.
- 2026-10-12 — Set-change practice: NEVER mid-soak; devnet
  rehearsal (R9) in the rc10 window; the G2-opening transition
  (drop akash + add first externals) is the first live practice.
  Phase-B epoch-based membership is the post-G2 roadmap; until then
  transitions are documented halt-and-restart ceremonies.
- 2026-10-12 — G2 opening is NOT a one-time gate. The ~20:00 UTC
  moment is close-out + announcement + the FIRST set change; the
  set stays fluid through G2 via additional set-change windows —
  operators join in BATCHES (each window = one ceremony: halt,
  fresh group key, coordinated restart), not one-per-validator.
  A validator joining in week 2+ of G2 is fully supported; the
  same ceremony in reverse handles departures/replacements
  (always preserving n >= 3f+1). Only pressure: don't run
  ceremonies so often they become the dominant operational risk —
  batch them.
