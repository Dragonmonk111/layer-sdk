# T12 soak close-out runbook

Soak started 2026-10-11 19:58 UTC (monitor: `g1-monitor.mjs`, log:
`soak-t12.log`, interval 30 s, stall-alarm 90 s).
Log of record: `g1-ceremony/321`.md. Interim stats already logged
(overnight block, 2026-10-12 08:05 UTC).

## Checkpoints

- **24 h checkpoint — 2026-10-12 19:58 UTC**: DEFAULT = continue to
  48 h. Abort early ONLY on: wedge (seat stuck >5 min), divergence
  (seat heights disagree >5), stall cascade (>10 consecutive
  alarms), or host reboot killing the monitor.
- **48 h close-out — 2026-10-13 19:58 UTC**: run below. (May extend
  to 72 h if all green, but 48 h + clean stats satisfies the gate.)

## At close-out

1. **Stats** (read-only, no chain interaction):
   ```
   node g1-ceremony/soak-stats.mjs g1-ceremony/soak-t12.log
   node g1-ceremony/soak-stalls.mjs g1-ceremony/soak-t12.log
   node g1-ceremony/g1-monitor.mjs --once
   ```
   Verify: no wedge/divergence across window, hourly deltas stayed
   in the ~3.4 s/blk structural band (no progressive trend), max
   inter-sample gap < ~2 min EXCLUDING the logged 2026-10-12
   monitor outage (11:26:26Z -> 11:53:31Z, ~27 min — session kill,
   chain unaffected; soak-stalls.mjs excludes it as a coverage gap
   automatically).
2. **Stop monitor**: FIRST disable the watchdog (it would otherwise
   resurrect the monitor within 5 min):
   `schtasks /Change /TN JunoClawG1MonitorWatchdog /DISABLE`
   then kill the node process (background command in the session
   that started it). `soak-t12.log` is the record — do NOT delete/
   rotate.
3. **T12 verdict** — PASS if: zero wedges (tip freeze >300 s), zero
   divergence, zero seat-down samples. Expected, pre-known
   structural findings: short tip-freezes (>=90 s; through 17.8 h:
   44 events, max 211 s, ~8% of window) from dead/degraded leader
   slots (akash stale lease = L0, home-vb 1-peer dial + host clock
   -83 s = L1); rate stable hour-over-hour. Report the freeze count
   and max from `soak-stalls.mjs`. Do NOT write "zero stalls >90 s"
   — the 08:05 overnight entry predates the reconciled definition
   (see `321`.md, 2026-10-12 stall reconciliation).
4. **T13 verdict** — ovh-uk1↔vahana skew ±100 ms (PASS, on-chain
   measured). home-vb HOST clock -83 s behind NTP (finding; NOT a
   future-drift violation; fix post-soak). akash unmeasurable (dead
   seat). Gate: PASS with findings.
5. **T14 verdict** — ovh-uk1 PASS (measured); home-vb PASS
   (provisioned, runtime unmeasured); akash PASS (by SDL spec);
   vahana PASS pending operator numbers (DM sent; if no reply, log
   explicit caveat, do not block G1 close).
6. **Append** final verdict block to `g1-ceremony/321`.md; update
   results list in `docs/G1_SAFETY_AND_TEST_PLAN.md`.
7. **Declare G1 closed** (if all gates PASS).

## Post-soak (rc10 window) — chain may now be touched

- rc10 fix list: 1-peer dial limit; by_height clobber; akash stale
  lease redeploy; resolver backfill.
- Coordinator host clock fix: `w32tm /resync` + fix time-service
  peers; re-run `w32tm /stripchart pool.ntp.org` (target <2 s);
  audit VBox guest time-sync (VBoxManage guestproperties /
  `/VirtualBox/GuestAdd/VBoxService/--timesync-set-status` etc.) so a
  host clock jump can never teleport the guest clock mid-run.
- Re-measure block rate after peering fixes — expect ~2 s/blk steady
  state; if not, dig leader-timeout tuning with full seat logs.

## G2 kickoff (after G1 declared closed)

- Merge site: Ravi's PR #1 branch (keep hero/narrative/status
  split) + add "Under the hood" chain section with G1 numbers.
- Publish: validator onboarding doc, endpoints page, milestone
  roadmap (LIVE/DEVNET/PLANNED status split everywhere).
- Send Anatoly the onboarding pack (draft:
  `g1-ceremony/DM-TO-ANATOLY.md`); first external validator seat.
- Chase Ravi's vahana ops numbers if still outstanding (draft:
  `g1-ceremony/DM-TO-RAVI-SITE-PR.md`).
