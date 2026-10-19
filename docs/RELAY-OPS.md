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

## status-bot (agent feed → #agents)

`/opt/buzz/status-bot.sh` (cron `*/30`): reads `jc-status:8082/api/status`
on the same host and posts a one-line `[OK]/[DEGRADED] tip … | probes
n/3 | uptime …` into `#agents` (`7b4f3765…`). Posts every run whose
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
