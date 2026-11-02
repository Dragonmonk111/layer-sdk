# Incident postmortem — 2026-11-01: certified-but-unfinalized parent livelock

G1 halted finalization at **h3938615** for ~7h. All 8 seats held a
certified parent that was never finalized; nodes correctly refused to
execute a merely-notarized ancestor and leaders livelocked through
~980k views. Recovery: coordinated all-stop + `data/consensus` archive
+ clean restart from the executed tip — no keys moved, no re-deal,
group key preserved. Finalization resumed ~14:52 UTC; full 8/8 mesh
by ~15:50. Post-recovery run clean: 26k+ blocks, digest parity across
seats.

## Timeline (UTC)

- **Oct 28 22:07** — coordinator opens `ssh -L 9090:127.0.0.1:9090` to
  OVH for monitor tooling. Forward held for 3d21h.
- **pre-incident** — `g1-monitor.mjs` polls `127.0.0.1:9090` every 30s,
  opening a **new HTTP/2 session each tick and never closing it**. The
  forward routes them to OVH's gRPC listener — ~2,047 ESTABLISHED
  sessions accumulate on OVH's sshd+slay3rd side.
- **~07:30** — full-CPU Docker compilation runs on the home-vb host
  (validator + compiler share the box). Validator margin, already thin
  (posthuman ~3.4h stale, 79anvi lagging), drops below the 6/8
  finality threshold mid-view.
- **wedge** — a payload is certified (notarized) but its Finalization
  activity never reaches quorum. Leaders keep proposing children of the
  certified tip; every seat's `verify()` rejects — "executed parent
  unavailable" — because the parent never executed. View churn forever.
- **~13:30–14:00** — coordinated recovery: all 8 seats `pkill slay3rd`,
  archive `data/consensus` → `consensus.bak-<ts>`, reply WIPED, hold
  until all eight :7001 ports verified refused, then restart.
- **14:52** — posthuman reconnects (stale state self-healed via
  `finality-backfill` — no force-sync needed), quorum-6 reached,
  finalization resumes. BlockPro joins → 7/8.
- **15:49** — vahana restarts anchored on h3938615/`464e35b2` → 8/8.
- **21:19** — anchor wallet `juno134zsp2…nraz` topped up via faucet
  (drip tx `C757D6EB…37C7`); its sequence cache had drifted 133 txs
  behind during the halt — faucet restart resynced.

## Root causes (layered)

1. **Monitor socket leak** — `g1-monitor.mjs` created HTTP/2
   connections per poll and closed none. ~2,047 sessions degraded the
   gRPC listener. Fix committed `88a3981`; standing rule: any poller
   must close sessions explicitly, fd-count audits in soak gates.
2. **Co-tenant resource starvation** — heavy compilation on a validator
   host. Policy: no non-validator CPU-heavy jobs on seats, ever.
3. **Thin effective quorum** — two seats already marginal; the faults
   landed on a 6/8-margin set, not an 8/8-healthy one.
4. **The wedge mechanism** — consensus correctly refuses to execute a
   certified-but-unfinalized parent (rc14 invariant; executing a dead
   variant is exactly what forked g1 at h2631739). Without
   finalization there is no safe way forward — by design. The missing
   piece was *recovery automation*, not the refusal.

## The `ssh -L` flood path (new finding)

The monitor's flood ran through a **laptop-side port forward** —
`ssh -L 9090:127.0.0.1:9090` (Oct 28, held 3d21h). Consequences that
made the incident worse than the socket audit showed:

- **fd cost split** — ~1,017 of the dead connections remained as
  CLOSE-WAIT corpses inside OVH's *sshd* session, invisible to any
  slay3rd-side socket count. Cleared when the tunnel was killed.
- **Loopback origin** — forwarded traffic arrives as `127.0.0.1`,
  bypassing the ufw donor-IP allowlist on :9090 entirely.
- **Origin hiding** — the flood's true source (laptop monitor) was
  masked behind the forward; from OVH it looked like local traffic.

**Standing rule:** `ssh -L` forwards to :9090/:8082 only for bounded
queries; kill the tunnel immediately after. Forwards are now counted
in soak socket baselines.

## What worked

- **Wipe-and-hold procedure** — deterministic, verifiable (all-ports-
  refused gate before restart), zero key movement. The dead
  certificate was never re-taught because gossip had nothing to teach.
- **`finality-backfill`** — posthuman's 3.4h-stale seat healed itself
  from donor history; no state-sync force needed.
- **Cross-seat digest parity** — h3938671 `44791ff5…` identical on
  OVH + home-vb verified during recovery.

## Fixes

| fix | status |
|---|---|
| Monitor rewrite (session-closing poller) | committed `88a3981` |
| Heavy-build ban on validator hosts | policy |
| **RC17v2** — in-place wedge self-recovery: archive `data/consensus` → `consensus.wedge-bak-*`, `wedge-recovered-<digest16>` marker (same digest never wiped twice), 2/hr cap, exit(75) for supervisor restart. 6/6 deterministic tests | built; `slay3rd:rc17v2-final`, sha `ce8834a2…db89d3`; staggered deploy Tuesday |
| **RC18** — protocol-level cure for certified-but-unfinalized deadlock | designed; `docs/RC18-CERTIFIED-PARENT.md`; upstream commonware question |
| Downtime alerting (>10min no `Block finalized`) | requirement restated; claw watchdog live |
| Faucet sequence resync on restart | verified live |

## Remaining risks

- RC17v2's marker dedupes the *same* dead digest — a re-taught variant
  stalls without auto-recovery (deliberate: wipe-loops are dangerous).
  RC18 is the real cure.
- Chain ran 6/8 for the final pre-recovery stretch — margin now 8/8,
  but two seats remain externally fragile (residential paths).
