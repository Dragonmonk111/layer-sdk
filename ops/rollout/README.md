# RC18 Fleet Rollout — Monday 2026-11-09 10:00 UTC

Coordinated, **non-rolling** upgrade of all 10 seats to `slay3rd-rc18v2`
(sha256 `133234bfa370f6531682c5746f9a859c31b98392612bde321d2c75c07c361f64`)
plus onboarding of breskulpeak (11 seats total).

Consensus-critical: rc18 changes vote-verification behavior (staged
verification of certified-but-unexecuted parents). rc17 seats **cannot**
participate in dead-variant escape — quorum must land on rc18 together.

## Per-seat procedure (each operator, inside the window)

```bash
# 1. Fetch this directory's scripts (or run from a checkout).
export BINARY_PATH=/path/to/slay3rd
export STOP_CMD='systemctl stop junoclaw-g1'      # your stop command
export START_CMD='systemctl start junoclaw-g1'  # your start command
export LOG_CMD='journalctl -u junoclaw-g1 -n 400 --no-pager'
export SEAT_NAME=ovh-uk1                        # your seat name
# optional: SMOKE_CONFIG=/path/to/sideport.toml for a boot test that
# does NOT join consensus

./fleet-rollout.sh
```

The script will: download + verify sha256 → smoke-run → back up to
`slay3rd.bak-rc17` → ask for confirmation → stop/install/start →
wait for `Consensus engine started` / `peer mesh` → print `SEAT` lines.

**Paste the `SEAT` lines into the fleet thread.** They look like:

```
SEAT ovh-uk1 4331200 4a2f4e4d498667c3
SEAT ovh-uk1 4331201 2fa034a3dd72318d
```

If your node is already on rc18, the script reports and skips the swap.

## Coordinator parity check (greenlight gate)

```bash
./fleet-parity.sh reports.txt --min-seats 11 --min-heights 3
```

`reports.txt` = all `SEAT` lines pasted by every operator.
- `PARITY-GREEN` — all seats agree on digest at ≥3 common heights → greenlight.
- `PARITY-RED` — a seat reports a different digest at the same height. That
  block is a **different block** even if app_hash matches. Investigate the
  diverging seat; do NOT greenlight.
- `PARITY-WAIT` / `PARITY-BLOCKED` — not enough data yet.

## If a seat reports a different height range

Ask the lagging seat to run:

```bash
./digest-report.sh <first-common-height> <last-common-height>
```

and paste the output — parity needs overlapping heights.

## Rollback

Per seat: `cp slay3rd.bak-rc17 slay3rd` + restart. Only inside the window
and only if the fleet agrees to abort — a mixed rc17/rc18 quorum behaves
exactly like today's chain (rc17 seats skip dead-variant verification).

## Watch list after cutover

- `staged_parent: simulated certified-but-unexecuted parent` — expected,
  healthy; counts staged rescues.
- `staged_parent: simulation of certified parent failed` — cold-path
  simulation failure; rare, bounded (once per process lifetime per digest);
  report if it repeats.
- `consensus parent certified but not executed` on an rc18 seat — fires
  when a dead-variant parent is first seen. On rc18 it should be followed
  by a `staged_parent:` line and recovery; if it appears repeatedly with
  no staged line, report it (staging didn't engage).
