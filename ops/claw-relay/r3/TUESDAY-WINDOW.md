# Tuesday window runbook — RESHARE + R3 attestation + relay v5 + RC17v2 + federation

Deterministic ordered plan. **The ceremony is ~90% async** — only the
apply step needs people live. Call reduced from ~2h to ~40 min.
Window: **Tue 15:30 UTC**. Havana + Lucky Star join as seats 8+9
(8→10 set, threshold 7/10; reshare only — never assemble-genesis).

## Async track (Mon–Tue morning — everyone in their own time)

**Critical path: joiner publics + IPs by Mon 12:00 UTC** — dealing
cannot start without them.

| when | who | what |
|---|---|---|
| Mon 12:00 | Lucky Star + ALXY | send public IP + PUBLIC keys (hot ed25519, BLS pub, MAYO pub). keys.json never leaves their host |
| Mon am | coordinator | ufw scoped rules for joiner IPs (:7001 p2p); test inbound from second host |
| Mon pm | coordinator | merge joiner publics → `share-requests/`; run `dkg-info` → publish `dkg-info.json` |
| Mon pm | all 8 + joiners | `export-pubkeys-only.sh` → pubkeys-only.json + fresh 32B nostr key each (send npub publics only) |
| Mon pm | all 8 dealers | `dkg-deal` → post `dealing-pub-*` + all `dealing-priv-*` to coordinator mailbox (sealed — safe to relay) |
| Mon eve | coordinator | redistribute each `for-<you>` priv file |
| Mon eve | all players | `dkg-ack` → send `ack-*` files to each dealer via mailbox |
| Tue am | dealers | `dkg-log` → post `log-*`; delete `dealer.seed` |
| Tue am | all players | `dkg-finalize` → stage `keys.json.g2` (NOT applied yet); `dkg-observe` → **report group-key prefix + validator table to group** |
| Tue am | all | stage `slay3rd` rc17v2 binary next to current (sha `ce8834a2…db89d3`), `node.toml` ready |

Everything above is per-operator ~15 min total, no coordination needed
beyond the mailbox. If dealing fails mid-track → new round number,
never reuse.

## The call — Tue 15:30 UTC (~40 min)

### Gate 0 (5 min)
Every seat reports the **same group-key prefix** from `dkg-observe` +
all 8 dealers confirm present. Dealer count ≥6 is the floor; **require
8 confirmations** — running the reshare at zero dealer slack is how
incidents start. Chain still finalizing, no wedge markers.

### Apply (15 min) — ONE restart does reshare + RC17v2
Each continuing seat, on their own GO: `keys.json` → `keys.json.g1.bak`,
`keys.json.g2` → in; binary → `.bak`, rc17v2 → in; **one restart**
applies both. Joiners start fresh with state_sync.
- Joiners: `snapshot adopted` → `consensus initialized` → `Block finalized`
- Verify: mesh 10/10, signers bitmap shows 10 bits, digest parity on a
  finalized height. New threshold **7/10** active from here.

### R3 attestation (10 min)
- 5 cw4 members submit `update_attestation` (msg shape per seat in
  `seat-roster.json`). Consensus-only seats (utsa, 79anvi, blockpro,
  Lucky Star, ALXY) → no tx; their relay npubs go to `--allowlist`.
- Verify: `{"list_active_seats":{}}` → **5 populated records**.
- `allowlist-add-claw.sh` the consensus-only npubs → restart claw.

### Relay flip + federation (10 min)
- claw-v5 (sha `6f6fafd9…858c6`) → `claw-relay.bak-<date>` → install.
  ExecStart: drop `--member-bindings`, add `--attestation-contract
  juno1rhlaj…t7qm` → daemon-reload + restart → expect
  `admission: refreshed N seats` (≥5 + break-glass merged) +
  unlisted-npub probe → `403 restricted`.
- OVH: `ufw allow from 57.129.103.229 to any port 7070`. Ravi restarts
  with `--peer ws://51.195.201.74:7070` → both logs show gossip connect.

### Verify + closeout (5 min)
- digest parity across ≥6 seats incl. both joiners
- daemon write + read round-trip on claw; status-bot next tick lands
- soak watchdog: stays until soak close (~16:45 Mon) — remove cron then
- Stage-5 divergence test: tomorrow (separate short session w/ Ravi)

## Attendance math (deterministic)

| present | dealing | cutover finalization | verdict |
|---|---|---|---|
| 8 old + 2 new | quorum 6 dealers, slack 2 | 10/10 ≥ 7 | full |
| 7 old + 2 new | 7 dealers ≥ 6 | 9/10 ≥ 7 | ok |
| 6 old + 2 new | 6 dealers = quorum, slack 0 | 8/10 ≥ 7 | **proceeds but flag: reschedule preferred** |
| ≤5 old | dealers < 6 | — | **impossible — reshare cannot run** |

Absent players (incl. absent old seats) aren't lost: their share is
revealed in dealer logs → they `dkg-finalize` later and rejoin
asynchronously. New-set slack after apply: up to 3 seats down and the
chain still finalizes.

## Post-soak branches (decided in advance — deterministic)

| soak outcome | action |
|---|---|
| Clean 24h | daemons stay on claw permanently; remove `soak-watchdog` cron; Buzz → cold standby; proceed to DNS prep |
| Any daemon write/read fail on claw | `sed 's|ws://127.0.0.1:7070|wss://relay.junoclaw.xyz|g'` on the affected script(s) only → next tick reverts → remove watchdog cron → note cause → retest before retry |
| claw-relay unstable | full sed revert on all six + remove watchdog + incident note; soak retries Tuesday post-window |

## Rollback surfaces preserved through the window

- Buzz containers untouched (docker compose stays up — watchdog's
  alert path until soak officially closes)
- `claw-relay.bak-<date>` binary + member-bindings mode config in git
- `keys.json.g1.bak` + binary `.bak` per seat — reshare rollback =
  files back + restart; new set can't finalize without 7 anyway, so a
  failed apply self-isolates to the seats that swapped
- Every sed swap is reversible by inverse substitution
- RC17v2 rollback = copy back previous binary per seat (no state wipe —
  the binary only adds the recovery path)

## What we need FROM joiners (Havana / Lucky Star / Alex Boy-ALX VUI)

Send each: `G1_OPERATOR_QUICKSTART.md` + `VALIDATOR_ONBOARDING.md` +
`G2-JOINER-INSTALL-SHEET.txt` + the release binary sha256. Collect back:

1. Their **peer IP** (for :7001 p2p + :9090 donor allowlists — we add
   scoped ufw rules, not open ports)
2. Their **generated key material** — they run `generate-testnet-keys`
   *locally*; `keys.json` never leaves their host; we need only the
   public export (`pubkeys-only.json` via export-pubkeys-only.sh)
3. Seat-index assignment — decided on the call before dealing starts.
   Havana + Lucky Star are IN Tuesday's reshare; ALX VUI onboards for
   the following window unless their materials land in time.

**Rule restated for joiner flow:** live-set membership changes are a
RESHARE ceremony (`dkg-info → deal → ack → log → observe → finalize`)
— never `assemble-genesis` (it mints a new group key = new trust root).

## Window sequence (Tue evening, all operators on call)

### Gate 0 — chain healthy
finalizing + digest parity across seats + no `wedge-recovered-*` markers
younger than the window. **Stop if any seat is mid-recovery.**

## Post-soak branches (decided in advance — deterministic)

| soak outcome | action |
|---|---|
| Clean 24h | daemons stay on claw permanently; remove `soak-watchdog` cron; Buzz → cold standby; proceed to DNS prep |
| Any daemon write/read fail on claw | `sed 's|ws://127.0.0.1:7070|wss://relay.junoclaw.xyz|g'` on the affected script(s) only → next tick reverts → remove watchdog cron → note cause → retest before retry |
| claw-relay unstable | full sed revert on all six + remove watchdog + incident note; soak retries Tuesday post-window |

## Rollback surfaces preserved through the window

- Buzz containers untouched (docker compose stays up — watchdog's
  alert path until soak officially closes)
- `claw-relay.bak-<date>` binary + member-bindings mode config in git
- Every sed swap is reversible by inverse substitution
- RC17v2 rollback = copy back previous binary per seat (no state wipe —
  the binary only adds the recovery path)

## What we need FROM joiners (Havana / Lucky Star / Alex Boy-ALX VUI)

Send each: `G1_OPERATOR_QUICKSTART.md` + `VALIDATOR_ONBOARDING.md` +
`G2-JOINER-INSTALL-SHEET.txt` + the release binary sha256. Collect back:

1. Their **peer IP** (for :7001 p2p + :9090 donor allowlists — we add
   scoped ufw rules, not open ports)
2. Their **generated key material** — they run `generate-testnet-keys`
   *locally*; `keys.json` never leaves their host; we need only the
   public export (`pubkeys-only.json` via export-pubkeys-only.sh)
3. Seat-index assignment — decided on the call before dealing starts.
   Havana + Lucky Star are IN Tuesday's reshare; ALX VUI onboards for
   the following window unless their materials land in time.

**Rule restated for joiner flow:** live-set membership changes are a
RESHARE ceremony (`dkg-info → deal → ack → log → observe → finalize`)
— never `assemble-genesis` (it mints a new group key = new trust root).
Joiners' window is separate; Tuesday only covers the 8-seat set.
