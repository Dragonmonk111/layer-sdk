# Buzz ↔ claw-relay parity plan

Written 2026-11-01 (g1 wedge day). Wire-compat verified at source level:
every ops daemon calls `buzz-cli` (kind-9 chat events, `h` channel tag,
NIP-42 auth) against `wss://relay.junoclaw.xyz`. claw-relay speaks the
same frame protocol (`EVENT`/`REQ`/`CLOSE`/`EOSE`/`OK`/`NOTICE` + NIP-42
challenge — the `["AUTH",challenge]` send exists *because* buzz-cli
requires it) and allowlists kind 9.

**Parity = daemons unchanged, `BUZZ_RELAY_URL` swaps.** Nothing below
touches daemon code except where noted.

## Daemon-by-daemon map

| daemon (ops/buzz/) | wire surface | parity status |
|---|---|---|
| `status-bot.sh` | `messages send` → `#agents` (7b4f…) | unchanged — env swap only |
| `proposal-watch.sh` | `messages send` → `#governance` (2a6c…) | unchanged |
| `buzz-vote-daemon.sh` | `messages get` REQ(#h,limit 50) + `send` → `#governance` | unchanged; **bot npub must be allowlisted/attested** — claw gates reads by signer too |
| `jc-alert-post.sh` | `send` → `#validator-ops` (810d…) | unchanged |
| `infra-monitor.sh` | `send` → `#agents` + `buzz-*` container checks | send unchanged; container check rewritten to claw-relay systemd/process check (see below) |
| `cw4-relay-watch.sh` | psql `pubkey_allowlist` sync | **deleted** — `--attestation-contract` makes admission a live view of `ListActiveSeats` (server-side cw4 join); no sync daemon needed |
| `allowlist-add.sh` | psql upsert | becomes an append to the claw `--allowlist` file — **local-only by design**: break-glass entries never federate |
| `transcript-anchor.sh`/`.py` | `messages get` window → merkle → `moultbook_v0` post | unchanged against claw-relay (reads via REQ); **claw-checkpoint is the successor** — run both during soak, retire .py after 7 clean anchor days |
| `backup.sh` | pg_dump + garage vol + configs | new claw variant: redb file snapshot + `/etc/claw/` + unit file tarball (see below) |
| Caddy `wss://` TLS | terminator in front of relay | unchanged — claw binds `127.0.0.1:7070` behind Caddy |
| garage (media store) | file attachments | **not used by any daemon** (all payloads are text) — Buzz-only surface; cutover unaffected. Revisit only if attachments ever become a requirement. |

## Channels (carry all three into `--channels`)

- governance `2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38`
- agents `7b4f3765-8494-4de8-bf7a-b2595da827e0`
- validator-ops `810d3c45-acb5-44f8-9155-8bc94b3a12b3`

## Deliberate differences (not bugs — flag in postmortem)

- **Reads are signer-gated** (`/query`, REQ replay) — Buzz served
  reads to any client; claw requires an allowlisted/attested signer.
  External observers get break-glass npubs explicitly.
- **No community scoping** — Buzz's allowlist was per-community
  (`host=relay.junoclaw.xyz`); claw is single-host by construction.
- **No NIP-09 deletes, no REQ shapes outside** `ids/authors/kinds/
  #tag/since/until/limit` — daemon audit below must confirm none are
  used (none seen in ops/buzz).
- **Break-glass allowlist does not federate** — proven 9/9 in the
  local divergence matrix; entries are per-relay-local forever.

## Live verification order (chain resumes → run these)

1. **Wire smoke (real buzz-cli, not the python probe):**
   `BUZZ_PRIVATE_KEY=<seat-nsec> BUZZ_RELAY_URL=ws://127.0.0.1:7070
   buzz-cli messages send --channel 7b4f3765-… --content "parity probe"`
   then `messages get --channel … --limit 5` — proves binary-compat,
   AUTH handshake, replay path.
2. **Attestation refresh:** relay logs `admission: refreshed N seats`
   from the real `ListActiveSeats`; a non-member probe gets
   `restricted: pubkey not allowlisted`.
3. **vote-daemon dry read** (no intents posted): `messages get` returns
   governance history — proves the REQ path daemons actually use.
4. **transcript-anchor.py against claw:** one manual run posts an
   `[anchor]` receipt — proves read-window + verify-command flow.
5. **Staging divergence:** OVH `claw-relay` + Ravi's vahana peer,
   `divergence-test.md` matrix (local matrix already 9/9 — this proves
   it over real gossip).
6. **24h daemon soak:** point `status-bot` + `infra-monitor` at claw
   (`BUZZ_RELAY_URL=ws://127.0.0.1:7070`), verify posts land and REQ
   reads return; then vote-daemon + proposal-watch.
7. **Checkpoint anchors:** `claw-checkpoint` hourly for 7 days —
   verify `refs` chaining on moultbook.
8. **DNS cutover:** Caddy backend swap only; Buzz binaries stay
   cold-standby 2 weeks (rollback = config revert, no data migration —
   claw keeps its own event history from first write).

## To write before Stage 6

- `claw-relay.service` systemd unit (Restart=always, `/opt/claw/data.redb`)
- `ops/claw-relay/backup.sh` — redb snapshot + config tarball, 14-day ret
- `infra-monitor.sh` variant checking `systemctl is-active claw-relay`
  instead of `docker ps … buzz-`
- `allowlist-add` doc note: file append + SIGHUP/restart semantics on claw
