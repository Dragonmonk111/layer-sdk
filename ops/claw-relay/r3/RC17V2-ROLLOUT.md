# RC17v2 fleet rollout — Tuesday window (paired with R3 attestation)

RC17v2 automates the 2026-11-01 wedge recovery: a certified-but-unfinalized
parent probed ~50x (~5 min) archives `data/consensus` to
`consensus.wedge-bak-<unix-ts>`, writes marker `wedge-recovered-<digest16>`,
and exits 75 for the watchdog to restart from the executed tip. Marker
dedupes per digest; filesystem-mtime cap = 2 recoveries/hour, survives
restarts; at cap the node stays wedged and logs ERROR for humans.

This is a RECOVERY automation, not the protocol cure — RC18 (sibling
proposals / finalized-parent fallback) is the real fix and is not in this
build.

## Artifact

- Final binary (built 2026-11-01, post-refactor + tests):
  `dist/slay3rd-rc17v2`, sha256
  `ce8834a275052ebea7fb05af7a4d1aa2a164668bd46f9b9330bed68cdbdb89d3`.
  Image pushed: `ghcr.io/dragonmonk111/slay3rd:rc17v2-wedge-recover-v2`
  (digest `sha256:3e361a0af9676543ed3210630bc91b5f977669197265fbe5061612e5baeee87f`).
  Extract: `docker create --name x <image> && docker cp x:/usr/local/bin/slay3rd .`
- Binary extract + `sha256sum` verify against the release asset is the
  deploy contract (rule: all seats take prebuilt artifacts only, never
  build on host).
- glibc floor 2.34 < OVH 2.35; target-cpu=x86-64-v2 (SSE4.2 — ymm opcode
  paths come only from runtime-dispatched deps, same recipe as deployed
  rc17; the patch itself adds no SIMD).

## Order — staggered, coordinator seats LAST

Per seat, in any order but never two-at-once on the same host:

1. Operator downloads binary, `sha256sum` vs release asset. Mismatch → stop.
2. Stop node (systemd `stop junoclaw-g1` / kill guard + slay3rd on home-vb).
3. `cp bin/slay3rd bin/slay3rd.bak-rc17` — keep the rollback binary.
4. Install rc17v2 binary, restart via the SAME supervisor (systemd or
   guard.sh). No manual `nohup` duplicates — one identity = one process.
5. Verify: `executed tip payload injected ... height=` matches the cohort
   tip; `peer mesh` deliveries resume; a `Block finalized` lands.
6. Only then next seat.

Rollback: `cp` the `.bak-rc17` back over, restart — no journal touch needed
(rc17v2 reads the same consensus journal; no format change).

## What to watch during rollout

- First 50-probe window on any seat = live fire of the new path. If a
  wedge fires during rollout itself, that seat exits 75 + restarts — this
  is the feature working, not a failure. Check for the marker file and
  the `LIVELOCK SELF-RECOVERY` error line.
- Watch for exit-75 restart LOOPS: a seat that re-learns a dead variant
  from a still-unupgraded peer may wedge again. The 2/hour cap bounds
  churn; >1 self-recovery fleet-wide in an hour = pause rollout and look.
- home-vb: guard.sh minute-cron handles the restart; no systemd. Restore
  cron AFTER the new binary is in place, in the same edit.

## Post-rollout verification (same window)

- `{"list_active_seats":{}}` returns the attested set (R3 step first).
- Fleet-wide digest parity at a fresh height — same finalized digest on
  every seat, not just same height (AGENTS rule 12).
- No `wedge-recovered-*` markers on any seat unless a real wedge fired.
