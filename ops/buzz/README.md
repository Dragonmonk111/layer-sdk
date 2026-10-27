# ops/buzz — relay + governance circuit scripts

The live copies run on ovh-uk1 under `/opt/buzz` (cron, see
`CRONTAB.txt`). This directory is version control for them — **edit
here, deploy there**, never the reverse. They were host-only until
2026-10-25 (bus-factor fix).

| File | Cron | What it does |
|---|---|---|
| `proposal-watch.sh` + `proposal-diff.py` | */2 min | polls the DAO proposal module via `127.0.0.1:8082/api/wasm/smart/`; posts `[dao:proposal]` events into `#governance` on any state change (new prop, vote-count change, status flip) |
| `buzz-vote-daemon.sh` | */2 min | watches `#governance` for `!vote yes|no|abstain <id>` signed by THIS seat's npub → casts ballot via `bls-relayer jc-execute` → re-queries `get_vote` before posting `[dao:vote]` + txhash. Committed state only, never "broadcast OK". |
| `cw4-relay-watch.sh` | */5 min | syncs relay `pubkey_allowlist` to the cw4 member list (DAO membership = channel access; `dao-member`-tagged rows only) |
| `status-bot.sh` + `status-format.py` | */30 min | jc-status → `[OK]/[DEGRADED]` heartbeat + state-change alerts into `#agents` |
| `infra-monitor.sh` | */15 min | disk/mem/container/node checks → `#agents` on state change + daily summary |
| `jc-alert-post.sh` | event (jc-status) | `JC_ALERT_CMD` sink — jc-status pipes each alert (finalized-height stall, seat absent >10 min, recovery) to `#validator-ops` |
| `transcript-anchor.sh` + `transcript-anchor.py` | hourly | Merkle-roots each channel's message window (`[from_ts, to_ts)` clock ranges) and anchors the root on-chain as a `moultbook-v0` entry (`Post.commitment`), chained to the previous anchor via `refs`. Posts an `[anchor]` receipt with a one-line verify command — anyone can reproduce the root from `buzz-cli messages get`. Makes deliberation tamper-evident before `claw-relay` exists |
| `backup.sh` | daily 03:30 | pg_dump + garage volume + configs → `/opt/buzz/backups/` (14-day) |
| `allowlist-add.sh` | manual | idempotent npub→allowlist upsert |
| `CRONTAB.txt` | — | snapshot of the ubuntu crontab that runs the above |

## Required host state (NOT in repo — keys stay on the host)

`/opt/buzz/state/`: `my-npub.hex`, `my-juno1.txt`, `proposal_addr`,
`status-bot.sec` (bot nsec, chmod 600), `members.json` (npub↔juno1
map, manual until the on-chain attestation contract replaces it),
`group_addr`, plus runtime state files (`*-seen`, `last-*-state`).
`/opt/dao-keys/dao-key.pem` — seat governance key.

## Deploying to another seat

Copy scripts + install cron entries; create per-seat `my-npub.hex` /
`my-juno1.txt` / `dao-key.pem`. One vote-daemon per seat — it only
acts on intents signed by its own npub. ⚠️ Known caveat: seats sharing
the relay identity npub collide (see docs/RELAY-OPS.md) — distinct
npub per seat is the fix.
