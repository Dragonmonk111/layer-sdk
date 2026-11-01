# Buzz ↔ claw-relay parity plan

Written 2026-11-01 (g1 wedge day). Wire-compat verified at source level:
every ops daemon calls `buzz-cli` (kind-9 chat events, `h` channel tag)
against `wss://relay.junoclaw.xyz`. buzz-cli uses **two** surfaces
(buzz/crates/buzz-acp/src/relay.rs):

- **WS** — requires `["AUTH",challenge]` on connect (`wait_for_auth_challenge`,
  else `NoAuthChallenge`); claw sends it and verifies the kind-22242 reply.
- **HTTP bridge** — writes go `POST /events` (+ `/query`, `/count`) with a
  NIP-98 `Authorization` header, **not** WS `EVENT`. claw serves all three
  routes (main.rs `route("/events"…)`). Kind 9 is in `ALLOWED_KINDS`.

Existing deployment: an older claw-relay shadow already runs on OVH at
`/opt/claw/` (127.0.0.1:7070, member-bindings admission). Step 1 below is
an **upgrade to v5** (`dist/claw-v5/`, attestation admission), not a fresh
install.

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

**Census finding (measured 2026-11-01):** kind-9 `h`-tag histogram on Buzz
is `7b4f…` 383, `bf2d0485-…` 229, `810d…` 104, `2a6c…` 30, `f497…` 1.
`bf2d0485-0e8e-4822-820f-f6cc11d9c408` is the **DM channel** — private
direct messages, no daemon dependency. Policy decision before cutover:
migrate it (add to `--channels`) or leave it Buzz-side. `f497…` is a
Welcome channel (1 event) — not migrated.

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

1. **Wire smoke (real buzz-cli, not the python probe): DONE 2026-11-01.**
   `buzz-cli messages send` → `accepted:true` (event `cd7341b9…`);
   `messages get` round-trips; `/count`+`/query` with NIP-98 → 200;
   unauthed reads → `missing Authorization`; random npub write →
   `403 restricted: pubkey not allowlisted`. Wire-compat confirmed on
   the member-bindings shadow (binary `edf8be0f…`).
2. **Attestation refresh:** relay logs `admission: refreshed N seats`
   from the real `ListActiveSeats`; a non-member probe gets
   `restricted: pubkey not allowlisted`.
3. **vote-daemon dry read: DONE 2026-11-01.** `messages get
   --channel 2a6c9ad2-…` returns history incl. a live transcript-anchor
   receipt — the REQ path daemons actually use works.
4. **transcript-anchor.py against claw:** one manual run posts an
   `[anchor]` receipt — proves read-window + verify-command flow.
5. **Staging divergence:** OVH `claw-relay` + Ravi's vahana peer,
   `divergence-test.md` matrix (local matrix already 9/9 — this proves
   it over real gossip).
6. **24h daemon soak: RUNNING — started 2026-11-01 ~16:45 UTC.**
   Decision: **Option A (all six daemons swapped to claw)** — the
   highway approach, exercises real daemon traffic end-to-end.
   Mechanism: daemons hardcode `BUZZ_RELAY_URL=wss://relay.junoclaw.xyz`
   inline — the swap is a single-token `sed` on:
   `status-bot.sh`, `proposal-watch.sh`, `buzz-vote-daemon.sh`,
   `infra-monitor.sh`, `jc-alert-post.sh`, `transcript-anchor.sh`.
   **Revert = sed back** (`ws://127.0.0.1:7070` → `wss://relay.junoclaw.xyz`);
   next cron tick reverts each daemon, no restart needed.

   Option C safety ported in: (a) relay moved off raw
   `run-relay.sh`/`pkill` into `claw-relay.service` (memory #65)
   **before** the swap — `MemoryMax=512M`, `CPUQuotaPerSec=500ms`,
   `TasksMax=256`, `LimitNOFILE=4096`, `Nice=10` verified via
   `systemctl show`; transitional member-bindings flags until
   Tuesday's attestation txs. (b) `soak-watchdog.sh` on root cron
   every 5 min — posts claw-relay/validator failures to **Buzz**
   (the path operators still watch during the soak), deduped by
   marker file. (c) Buzz containers untouched — rollback surface
   preserved. Deployment wrinkle found: `/opt/claw` was root-owned
   from the raw-process era; `chown -R ubuntu` applied (unit runs
   `User=ubuntu`, least privilege).

   First 45 min verified: status-bot 17:00 tick landed on claw
   (`#agents`, a748d469, ts 1793552402), vote-daemon `synced.json`
   refreshed 17:30, transcript-anchor ran against claw 17:17.
   **Anchor chain commits are failing `InsufficientFunds` on the
   anchor wallet (`juno134zsp2…nraz`) — pre-existing since the halt,
   NOT a soak regression; wallet needs a top-up.**

   Exit criteria: 24h clean, or any claw-side write/read failure →
   revert sed + note the cause. Socket-baseline gate (#141): count
   `:8082`/`:9090` conns at soak start and +1h — flat required.
7. **Checkpoint anchors:** `claw-checkpoint` hourly for 7 days —
   verify `refs` chaining on moultbook.
8. **DNS cutover:** Caddy backend swap only; Buzz binaries stay
   cold-standby 2 weeks (rollback = config revert, no data migration —
   claw keeps its own event history from first write).

## Gap: bot identity does not come from chain state

Every bot post — `status-bot`, `proposal-watch`, `jc-alert-post`, and the
vote-daemon's `[dao:vote]` confirmations and reads — is signed with
**one** key, `status-bot.sec`. That npub is not a seat, so it is never in
`ListActiveSeats`. On claw it is admitted only via the local `--allowlist`
file. Gossip ingress runs the same `admit_event` as client writes
(gossip.rs `handle_remote`), so bot events federate **only to relays whose
operator lists the bot npub locally**.

- Single-relay cutover (OVH only): at parity with Buzz (bot was a manual
  `pubkey_allowlist` row there too). **Not a cutover blocker.**
- Multi-relay (vahana, observers): a relay that hasn't listed the bot sees
  no `[dao:proposal]` / `[dao:vote]` / alerts. Interim fix: the bot npub
  goes into each relay's runbook allowlist. **Principled fix:** an
  `--agent-registry` admission source (agents registered on-chain against
  a DAO member are admitted federation-wide). This matches the
  architecture rule that agent-registry is the source of operational
  identity. Track it as an RC item; don't hand-maintain lists.
- Also split the key: one bot identity per function (status/gov/alerts),
  so a compromised or rotated bot key has a bounded blast radius.

## Census gates — RESULTS (measured 2026-11-01, `buzz-postgres.events`)

Buzz accepted kind=1 and non-member writes after NIP-42 auth (memory
#99). Ground truth from the live DB:

1. **Kind census — done.** 19 kinds present. Only **kind 9 (746 events)
   is daemon-load-bearing** — it is in `ALLOWED_KINDS` ✓. Everything else
   is Buzz's social/moderation surface with zero daemon dependency:
   39000/39001/39002 (57, NIP-29 group metadata — relay-side, not ported),
   30177/40099/44100 (81, Buzz platform customs), 0/1/7 (26, social),
   9000/9005/9007/9021 (29, NIP-29 moderation), misc ~11.
   **No writer breaks at cutover** — those kinds retire with the social
   surface. Document as deliberate-deny, not a parity miss.
2. **Author census — done.** Top writers: `a748d469` (499, kind 9 —
   status-bot/infra key), `4a8189fc` (259, kind 9 + moderation — second
   bot key), `d121b7da` (113, platform/group events — **Buzz system
   identity, not a seat**), `c274b3eb` (47), `56e611d3` (15), tail ~6.
   Confirms the bot-identity gap: the two heaviest authors are shared
   non-seat keys → both go in the local `--allowlist` at cutover.
3. **Client census — still open.** Caddy access log for
   `relay.junoclaw.xyz`, distinct user-agents + paths over 7 days. Any
   human client (web/desktop app) that isn't plain NIP-42/NIP-98 needs a
   named replacement before DNS. Daemons alone are not the population.
4. **Limits:** `MAX_REPLAY=2000` ≥ vote-daemon `--limit 50` ✓;
   `MAX_CONN_PER_IP=16`. Check that Caddy forwards the client IP, or
   every client behind Caddy counts as 127.0.0.1 and shares one 16-cap.

## Validator-host safety gate (2026-11-01 lesson — the relay shares OVH with idx5)

The wedge started with an unbounded client aimed at a validator process.
claw-relay runs on the same host and polls the chain API
(`:8082` → slay3rd gRPC):

- Admission client is bounded by construction: a reqwest `Client` per
  query with a hard `HTTP_TIMEOUT`, dropped on return (admission.rs
  `smart_query`), no pool carried across refreshes, 60s default interval.
- **Measure anyway:** `ss -tn | grep -cE ':8082|:9090'` at soak start and
  again 1h later. Flat or it doesn't proceed.
- systemd unit caps: `MemoryMax=512M`, `CPUQuota=50%`, `Nice=10`,
  `TasksMax=256`. The relay must never be able to starve slay3rd.
- Replace `run-relay.sh` + `pkill -f` restarts (memory #65 footgun) with
  the unit file.

## History and anchor continuity

- **Archive of record:** final Buzz `pg_dump` + final `transcript-anchor`
  receipt, frozen and hash-pinned. Buzz history stays verifiable against
  its on-chain moultbook roots forever; no import is required.
- **Optional import:** Nostr events are self-authenticating, so Buzz
  events can be replayed verbatim into claw (same ids → vote-daemon
  `*-seen` files stay valid; `get_vote` pre-check prevents re-voting
  anyway). Decide deliberately; don't do it implicitly.
- **Anchor chain must not fork at cutover:** the first `claw-checkpoint`
  anchor `refs` the last Buzz `transcript-anchor` post id, so the
  tamper-evident chain is one continuous line across relays.
- **Rollback asymmetry:** events written to claw after DNS don't exist in
  Buzz. Rollback is DNS-only, but say in the rollback note that the
  claw-era history lives in claw's redb and its anchors.

## Process lifecycle table (rule: purpose + kill condition + expiry at creation)

| process | purpose | kill / expiry |
|---|---|---|
| claw-relay v5 (systemd) | authoritative relay | permanent after cutover |
| claw-checkpoint cron | hourly anchors | permanent after cutover |
| `BUZZ_RELAY_URL` overrides on soak daemons | Stage 6 soak | made permanent or reverted at DNS cutover, same day |
| `soak-watchdog.sh` (root cron */5) | alerts claw failures to Buzz | removed when soak ends (revert or cutover) |
| transcript-anchor.sh (Buzz) | parallel anchors during soak | removed after 7 clean claw anchor days |
| socket-count check | safety gate | one-shot commands, never a loop |
| Buzz containers | cold rollback | `docker compose down` +14 days after DNS; volumes kept until archive of record verified |

## Stage-6 ops artifacts — written (commit 27c7849)

- `claw-relay.service` — systemd unit, attestation admission, resource
  caps (MemoryMax=512M, CPUQuota=50%, Nice=10), binds 127.0.0.1:7070.
- `backup.sh` — redb snapshot + config tarball, 14-day retention.
- `infra-monitor-claw.sh` — `systemctl is-active claw-relay`/`junoclaw-g1`
  checks replace the `docker ps … buzz-` container checks.
- `allowlist-add-claw.sh` — bech32-verified append; allowlist loads once
  at start so every add needs `systemctl restart claw-relay`.
- `soak-watchdog.sh` — 5-min cron during the soak: posts claw-relay
  inactive / validator inactive / stale claw health to **Buzz** (the
  operator-visible channel while daemons point at claw). Marker-file
  dedupe; expires with the soak.
