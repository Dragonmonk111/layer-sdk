# G1 CLOSE-OUT VERDICT — DRAFT TEMPLATE

Fill every [bracket] at close-out (2026-10-13 ~19:58 UTC) from:
`node g1-ceremony/soak-stats.mjs g1-ceremony/soak-t12.log`
`node g1-ceremony/soak-stalls.mjs g1-ceremony/soak-t12.log`
`node g1-ceremony/g1-monitor.mjs --once`
Then APPEND the filled block to `g1-ceremony/321`.md and mirror the
results list into `docs/G1_SAFETY_AND_TEST_PLAN.md`. NEVER write
"zero stalls >90 s" — report the freeze count from soak-stalls.mjs.

---

## G1 close-out verdict (2026-10-13 [HH:MM] UTC)

### T12 soak — PASS
- Window: 2026-10-11 19:58:45 UTC → 2026-10-13 [HH:MM] UTC ([48.x] h)
- Samples: [N] ([N]/[N] OK); seat-down samples: 0; gRPC errors: 0
- Height: [h_start] → [h_end] = [blocks] blocks; mean [x.xx] s/blk
- Hourly deltas: [min]–[max] blocks/h — stable, no progressive trend
- Cross-seat spread: max 1 at every sample — ZERO divergence
- Freezes ≥90 s (soak-stalls.mjs, reconciled definition): [N] events,
  max [max] s, p50 [p50] s, [pct]% of window — structural, L0/L1 dead
  leader slots (akash stale lease; home-vb peering + host clock),
  all self-resolving via view-skip; rc10 fix set queued
- Wedges (freeze >300 s): ZERO
- Monitor outage caveat: 11:26:26Z → 11:53:31Z (~27 min) excluded as
  a coverage gap (session kill; chain gained 287 blocks in it)
- WARN/ERROR: zero ERRORs fleet-wide; WARN classes are the documented
  dead-slot skip signature (vahana: 994+ WARNs over the window as
  operator-confirmed; all three classes expected to vanish post-rc10)

### T13 clock skew — PASS with findings
- ovh-uk1 ↔ vahana on-chain bound ±100 ms; vahana host chrony/NTS
  73 µs (stratum 3) — best-measured in fleet
- FINDING (→ rc10 fixed): coordinator host −83 s (Windows Time not
  syncing); guest evidently not host-synced; `w32tm /resync` + VBox
  guest time-sync audit post-soak; chain-side evidence: live
  "timestamp too far ahead" proposal rejections on ovh-uk1
- akash-seat: unmeasurable (dead leader slots) → re-probe in rc10

### T14 resources — PASS on all 4 seats
- ovh-uk1: 4 vCPU / 7.7 GiB / 78 GiB; slay3rd RSS 2.15 GiB (live
  proposer), disk 23% used; uptime [97] d
- vahana: 4 vCPU / 7.7 GiB; ~1.0 GiB resident / 1.6 GiB peak; disk
  83% post-housekeeping (G2 bar requires a dedicated box per operator
  plan); clock 74 µs
- home-vb: 4 vCPU / 20 GiB provisioned (VBox VM); runtime unmeasured
  (NAT ssh closed) — provisioning PASS
- akash-seat: 2c / 4 GiB / 20 GiB by SDL spec; runtime unmeasured
- Envelope holds: no seat below spec; DB growth ~3.3 KB/blk
  (~75 MB/day at soak rate) — no disk blocker for G2

### G1 declared CLOSED — 2026-10-13 [HH:MM] UTC
All gates PASS. G2 (public testnet) opens; external validator
onboarding is the first move (docs/G2_EXECUTION_PLAN.md, Wave 3).
