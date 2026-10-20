# Relay ops — relay.junoclaw.xyz (Buzz on OVH)

Production Nostr relay for validator ops, governance, and agent feeds.
Buzz `0.2.1` speaks NIP-29 groups + NIP-42 auth natively; the relay URL
is the community boundary (`relay.junoclaw.xyz` = one community).

## Topology (all on ovh-uk1, 51.195.201.74)

| Service | Container | Access |
|---|---|---|
| buzz-relay | `ghcr.io/block/buzz:0.2.1` | `127.0.0.1:3000` → Caddy TLS → `wss://relay.junoclaw.xyz` |
| buzz-postgres | postgres:17 | internal only (`buzz` db / `buzz` user) |
| buzz-redis | redis:7 | internal only |
| buzz-garage | garage:v2.0.0 | internal only (S3 media backend) |
| caddy | host service | `/etc/caddy/Caddyfile` → `reverse_proxy 127.0.0.1:3000` |

Compose project: `/opt/buzz/compose.yaml` (project `buzz-relay`).
Memory caps: relay 768M, postgres 512M, garage 256M. Garage lacks
linearizable conditional writes → **git-repo CAS hosting is disabled
on this backend** (`BUZZ_GIT_CONFORMANCE_PROBE=false`); chat/media/
channels unaffected. Swap in CAS-capable S3 before enabling git paths.

Relay identity pubkey: `d121b7dab5ace10a3b4eab5254d3e95b8b2e4db999a498f8da18407577600a5c`
(env `BUZZ_RELAY_PRIVATE_KEY` in compose — never commit).

NIP-11 advertises `auth_required: true`, `restricted_writes: true` —
pubkey allowlist gates everything.

## Key ops commands

```bash
# allowlist a member (npub -> hex -> upsert, idempotent)
/opt/buzz/allowlist-add.sh npub1... 'operator name'

# list current allowlist
docker exec buzz-postgres psql -U buzz -d buzz -tc \
  "SELECT encode(pubkey,'hex'), note FROM pubkey_allowlist"

# channels are relay-owned (kind:9007 signed by the relay key):
buzz-cli channels create --name <ch> --type stream --visibility open \
  BUZZ_PRIVATE_KEY=<relay key> BUZZ_RELAY_URL=wss://relay.junoclaw.xyz
```

Ops channels (created 2026-10-19, relay-owned):
`#validator-ops` `810d3c45…`, `#governance` `2a6c9ad2…`,
`#agents` `7b4f3765…`.

## cw4 → allowlist sync (DAO-owned membership)

`/opt/buzz/cw4-relay-watch.sh` (cron `*/5`): reads the cw4 group member
list via the jc-status wasm passthrough and syncs `pubkey_allowlist`
to it. Activation steps, in order:

1. Instantiate Agents DAO → write group addr:
   `echo juno1<group...> | sudo tee /opt/buzz/state/group_addr`
2. Populate the binding map `/opt/buzz/state/members.json`:
   `{"juno1addr": "<npub-hex>", ...}` — v1 manual file; v2 reads the
   on-chain hot↔cold attestation instead.
3. Watcher syncs adds/removes tagged `note='dao-member'` only —
   manual entries (relay key, agents) are never removed.

## Backups + restore

Cron daily 03:30 → `/opt/buzz/backups/` (14-day retention):
`pg-*.sql.gz` (pg_dump), `garage-*.tar.gz` (volume), `config-*.tar.gz`
(compose + garage.toml + Caddyfile + /srv/www).

Restore (fresh host): install docker → `docker compose up postgres
garage` → `zcat pg-*.sql.gz | psql -U buzz buzz` → `docker run --rm -v
buzz-relay_garage-data:/data -v $B:/in alpine tar xzf /in/garage-*.tar.gz
-C /data` → compose up all → repoint DNS. DNS-only (grey cloud) is
required for Caddy issuance.

## Infra monitor + storage policy (ovh-uk1)

`/opt/buzz/infra-monitor.sh` (cron `*/15`): checks disk %, available
memory, buzz container states, `junoclaw-g1` service, backup
freshness. Posts to `#agents` on state CHANGE only (deduped via
`state/last-infra-state` — alert on degradation, "recovered" on
return), plus a daily 06:00 summary post (deduped by date flag).

Storage management deployed 2026-10-19 (disk 28% → 21%):

- journald capped at 500M (`/etc/systemd/journald.conf.d/
  99-junoclaw-size.conf`) — was 3.9G unbounded
- docker log rotation for ALL containers: json-file 10m × 3
  (`/etc/docker/daemon.json`; brief restart applied it — compose
  uses `restart: unless-stopped` so the bounce self-heals)
- weekly image prune (cron Sun 04:00)
- backups already 14-day retained
- removed 5.6G stale `~/go/pkg` module cache (leftover; go not
  installed on the host)

Alert thresholds: disk ≥ 85%, mem avail < 1GB, any buzz container
down, node inactive, backups > 25h stale. `/data` (validator
rocksdb) is the growth consumer — bounded by pruning; the daily
summary tracks it.

## status-bot (agent feed → #agents)

`/opt/buzz/status-bot.sh` (cron `*/30`): reads `jc-status:8082/api/status`
on the same host, formats via `/opt/buzz/status-format.py`, and posts a
one-line `[OK]/[DEGRADED] seq … | probes n/3 | uptime …` into `#agents`
(`7b4f3765…`). Posts every run whose
minute < 30 (twice/hour heartbeat) plus immediately on any state change
(degraded/recovered). Signing key is a dedicated bot nsec at
`/opt/buzz/state/status-bot.sec` (chmod 600) — its pubkey is
allowlisted with `note='status-bot'`. Binary: `/opt/buzz/bin/buzz-cli`
(linux build; `buzz messages send --channel <uuid> --content -` reads
stdin).

To rotate the bot key: `openssl rand -hex 32 > status-bot.sec`, derive
the x-only pubkey, update the `pubkey_allowlist` row, delete
`state/last-bot-state`.

## Blast-radius note

Relay is colocated with the ovh-uk1 validator seat. Workload is tiny
today; if media/db growth or CPU contention ever threatens the seat,
migrate: dump → stand up on dedicated host → DNS cutover → old stack
down. Postgres+garage are the only state that matters.

## Governance loop (proposal-watch + vote-daemon)

Two cron jobs close the chain↔relay governance circuit:

- **`/opt/buzz/proposal-watch.sh`** (every 2 min): polls the
  dao-proposal-single module via the local API
  (`127.0.0.1:8082/api/wasm/smart/<proposal_addr>`) and posts
  `[dao:proposal] #N STATUS …` events into #governance
  (`2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38`) on any state change — new
  proposal, vote-count change, status flip. State:
  `state/proposals-seen.json`, diff logic in `/opt/buzz/proposal-diff.py`.
- **`/opt/buzz/buzz-vote-daemon.sh`** (every 2 min, per-seat): polls
  #governance for `!vote yes|no|abstain <id>` intents authored by
  **this seat's** npub (`state/my-npub.hex`), checks the seat's existing
  ballot via `get_vote`, casts `{"vote":…}` via `bls-relayer jc-execute
  --jc-key-hex` (raw PEM scalar — NOT a KDF seed), re-queries the ballot
  to confirm commit, then posts `[dao:vote]` confirmation with the
  txhash. Duplicate intents are ignored with an "already voted" notice.
  State: `state/vote-daemon-seen`, log `state/vote-daemon.log`.
  Required state files: `my-npub.hex`, `my-juno1.txt`,
  `dao-keys/dao-key.pem`, `status-bot.sec`, `proposal_addr`.

To run the daemon on another seat: copy the script, set `my-npub.hex` /
`my-juno1.txt` to that seat's bound pair, point `dao-key.pem` at the
local governance key. One daemon per seat; it ignores other members'
intents by pubkey.

⚠️ Shared-npub caveat: home-vb and ovh-uk1 currently share the relay
identity npub (`d121b7da…`, the `RELAY_PRIVATE_KEY` in `.env`). An
intent from that npub executes on whichever daemon reads it — each
cast under its own seat's key. Fine while both seats are one operator;
give each seat a distinct npub before independent agents vote.
