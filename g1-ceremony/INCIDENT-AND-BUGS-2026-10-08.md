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
