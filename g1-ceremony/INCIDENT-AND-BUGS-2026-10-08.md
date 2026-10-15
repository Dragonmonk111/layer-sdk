# G1 Launch Incident + rc2 Bug List — 2026-10-08

Chain: `junoclaw-g1`, binary `slay3rd v0.6.0-rc1`. Timeline UTC.

## Outcome

Chain LIVE since ~18:12, finalizing ~3.2 blocks/s (0.31 s/blk) at 3-of-4
voting (ovh-uk1, vahana, akash-seat). h≈17,700+ by 20:50. T1 PASS, T2 PASS
(byte-identical proposal/cert/payload at sampled heights across all gRPC
seats, `g1-agreement.mjs`), T5 PASS (multiple restart/backfills), T3/T12
soak running (`g1-run.jsonl`).

## Incident summary (17:20–18:12)

1. First launch (16:47) wedged at h=4: notarized-but-unfinalized block +
   propose-skip livelock (parent-execution gate + 2 s window).
2. Debugging restarts poisoned freshly wiped nodes: seats replayed stale
   notarization certs (persisted in `/data/consensus`, re-broadcast via
   P2P) — freshly wiped nodes re-adopted the dead view state.
3. Clean relaunch required: ALL seats' disks wiped simultaneously, pod
   restarted with `RESTART_EPOCH=2`, then 4 fresh seats converged at
   view 1 → block 1 finalized with 3-of-4 (vahana late).
4. Chain took off only when the akash pod rejoined as 3rd voter (~18:12).

## rc2 bugs (with evidence)

1. **Propose-skip livelock after notarized-but-unfinalized block.**
   Log: `propose: parent not executed in time — skipping proposal for
   this view parent=cac2997… view=1141` repeating forever. No nil-round
   finalize path escapes it. Repro: any leader miss during a notarize
   window at 3 voters.
2. **Stale notarization certs poison freshly wiped nodes.** Persisted in
   `/data/consensus`; a single old node re-broadcasts them; wiped nodes
   boot at `current_view=1141` (not 1) and re-enter the livelock.
   Wipe must be simultaneous network-wide — unsafe operationally.
3. **Late joiners can never vote.** `No stored payload for resume
   height — node will not vote`. No P2P cert replay; the only escape is
   gRPC state-sync adoption, which requires donor 9090 reachability AND
   a fresh app state.
4. **No payload relay cascade.** Proposers push payloads to their direct
   peers only. A seat without a direct link to the proposer misses the
   push and must fetch (2 s re-ask). At 3.2 blk/s production this forces
   a systematic execution deficit on the poorly-linked seat.
5. **Backfill span ceiling wedges drifting nodes permanently.**
   `BACKFILL_MAX_SPAN=512`: once tip − executed > 512 the backfill
   refuses to bridge, relay pushes are lookahead-dropped (>64), and
   `finalize: finalized payload unavailable — execution halted until
   it is fetched` repeats forever (h=3515 wedge; Recv-Q 537 KB unread
   while gRPC stays responsive — the halt loop appears to block the
   P2P reader/pipeline). Combine with bug 4: permanent wedge.
6. **akash pod inbound port dead.** `196.216.137.249:30823` never
   accepts (from any seat). Pod works outbound-only. `deployment
   update` cannot add endpoints to a live lease (on-chain group spec) —
   needs a fresh deployment for rc2, or accept outbound-only.
7. **Reader/pipeline stall under backlog** (may be 5's mechanism):
   kernel Recv-Q grows to 500 KB+, process idle at 0.4–4.8% CPU, gRPC
   responsive, consensus frozen. Needs a debug-level repro.

## Non-bug findings

- **Windows host clock +77 s** vs NTP (monitor `tip-age` shows +78 s
  constant offset). Fix elevated `w32tm /resync` (service stopped).
  WSL clock itself is correct (matches ovh).
- home-vb seat: `broadcast to 2 peers` — links to ovh+vahana fine,
  pod link missing both directions (see 6). State-sync adoption from
  ovh+vahana donors works: adopted h=16136→executed to live edge in
  ~90 s, proposed its own block at view 22937, finalized (its proposal
  got quorum). Wedged later per bug 4+5.
- State-sync donors MUST be gRPC-reachable from the joiner; home-vb's
  egress NATs to the coordinator IP so ovh's 9090 ACL admits it.
- Snapshot offers are fresh (near-tip, 640 KB, 1 chunk) — `ListSnapshots`
  cache is per-commit. Root `51165397…` identical across donors.

## Status of seats (20:50 UTC)

| seat | state |
|---|---|
| akash-seat | voting, log-only (no gRPC exposure) |
| ovh-uk1 | voting, h at tip, NTP ok, 380 MB RSS |
| vahana | voting, h at tip (host checks: morning, Ravi) |
| home-vb | committed observer h=17193, wedged (bugs 4+5), gRPC serving, snapshot offering |

## Tomorrow (rc2 day) — suggested order

1. Fix nil-round finalize path (bug 1) — the original wedge.
2. Wipe-not-required fix for cert replay (bug 2) — epoch bump on wipe.
3. Relay cascade or backfill-span removal (bugs 4+5) — the drift wedge.
4. Fresh akash deployment with 9090 exposed (bug 6) — full 4/4 mesh.
5. Re-run T4 (leader kill), T6 (two-down halt) once home-vb votes again.
6. T7–T11 (tx, wasm, airdrop, lightclient) against the live chain.

---

# Post-G1 3-of-4 Threshold Stress Test — 2026-10-14/15

Chain: `junoclaw-g1`, binary `slay3rd v0.6.0-rc9`. Timeline UTC.
After G1 closeout the Akash seat lease was closed, leaving a deliberate
**3-of-4 running configuration** — a live stress test of the chain at
exactly threshold. Findings below are from that configuration.

## Outcome

- Stall window: **13:51:05 UTC Oct 14 → ~04:13 UTC Oct 15** (~14.4 h).
  Last finalized height **1,295,875** (view 1,768,129).
- Trigger: the home-vb seat panicked on journal prune
  (`BlobMissing`); with Akash already offline, voting dropped to
  2-of-4 — below the 3-of-4 threshold — and finality halted.
- Recovery: plain process restart, journals intact — no wipe, no
  ceremony. The seat replayed its journal past the internal gap and
  resumed voting. Chain finalizing again past h=1,366,000; a faucet
  drip pending in a peer mempool from before the stall finalized at
  h=1,303,605 minutes after recovery, plus a second drip landed.

## Timeline

| UTC | Event |
|---|---|
| ~Oct 12 | Akash seat (`196.216.137.249:30823`) closed — 3-of-4 stress config begins (ovh-uk1, vahana, home-vb voting). |
| 13:51:05 Oct 14 | home-vb `g1-node` panics at `simplex/actors/voter/actor.rs:229`: `unable to prune journal: Runtime(BlobMissing("slay3r-junoclaw-g1", "00000000001afab5"))`, then `resolver should not finish` (`engine.rs:237`). Voter + resolver die; executor channel closes. |
| 13:51→ | ovh + vahana keep building/relaying proposals (`peers=2`) but no cert forms — every view times out. ~45% CPU spinning nullifies for 14 h. |
| ~22:10 Oct 14 | WSL `Ubuntu-24.04` reboots; `g1-node` was a transient `systemd-run` unit → unit definition lost, no auto-restart. |
| 04:13 Oct 15 | Seat restarted under a persistent unit; journal replayed past the gap, caught up ~4,600 views in seconds, quorum restored. |
| 04:29 Oct 15 | Pending faucet drip finalized at h=1,303,605 (`tx_count=1`). |

## Findings → fixes implemented

1. **Non-idempotent journal prune (bug, fixed in-tree).**
   `commonware-storage` `segmented/manager.rs::prune` propagated
   `BlobMissing` when a section file was already absent on disk —
   an earlier interrupted prune removed the file without advancing
   the cursor, so the next prune hit the missing blob, panicked the
   voter, then killed the resolver (`resolver should not finish`).
   Journal showed a single-blob interior gap (`…1afab5` missing inside
   `…1afab6`–`…1afac1`); disk had 930 GB free — not resource pressure.
   **Fix: vendored `commonware-storage` at `lib/commonware-storage`
   (crates.io `[patch]`); `prune` now treats `BlobMissing` as
   already-pruned and continues.** Verified `cargo check -p slay3rd`
   clean.

   **Upstream status (Oct 15):** reported as
   `commonwarexyz/monorepo#5266`; closed NOT_PLANNED with reference to
   declined PR #4196. Commonware's stated contract: **any external
   storage manipulation is undefined behavior and any storage error is
   fatal by design.** Their position is that a single correctly-run
   node can never see this gap (remove+sync runs lowest→highest, and
   the blob map is rebuilt from storage on restart). Our most plausible
   trigger was the stale duplicate workloads sharing the data dir
   (see finding 3) — i.e. we violated the no-external-manipulation
   assumption rather than hitting an internal race. The vendored patch
   stays as defense-in-depth, but the operational invariant stands:
   **never run two node processes over one data dir, never touch data
   files while the node runs.**

2. **Transient systemd units = no watchdog.** `g1-node` ran under
   `systemd-run` — no `Restart=` policy, and the unit definition
   vanished on WSL reboot, so a crashed seat stayed down 14 h with
   no alert. **Fix: `/etc/systemd/system/g1-node.service` persistent
   unit with `Restart=always`, enabled at boot.** OVH side already
   had `Restart=always`.

3. **Zombie clone seat.** `g1-seat-clone` (index 0, port 7002,
   `/data-seat`) kept running but spun on `nullify View(1)` with
   handshake rejections — stale Oct-10 experiment, dead weight.
   **Stopped.**

4. **No stall alerting.** `jc-status` tracked tip age in the UI only;
   nobody was paged during the 14 h window. **Fix: `jc-status` now
   POSTs a JSON webhook (`JC_ALERT_WEBHOOK` env) when the chain tip
   hasn't advanced for >120 s, and a recovery notice when finality
   resumes.** Also fixed `tip_seen_at` semantics — it now only updates
   when the tip *advances*, so a stalled chain correctly reports
   DEGRADED instead of staying green.

5. **Stale R9 devnet containers** left running 16 h on the host —
   stopped.

## Evidence

- Crash tail + journal manifest: `home-vb:/opt/g1-node/incident-2026-10-14/`
  (`slay3rd-crash-tail.log`, `journal-blobs.txt`).
- Panic frames: `voter/actor.rs:229` (prune `.expect`), `engine.rs:237`
  (resolver finish), then `Executor channel closed`.
- OVH journal: proposals for h=1,295,876 relayed to `peers=2`, no
  certificate from 13:51:05 onward.

## Lesson for G2

The 3-of-4 config is fragile by construction — one fault from halt.
The BFT itself behaved correctly (halted rather than finalizing wrong
state); the failure was operational: no restart policy, no alert. For
G2 (n=5, threshold 4): external seats get the persistent-unit +
`restart_anchor` runbook, and `jc-status` alerting is the tripwire
that replaces "someone notices."
