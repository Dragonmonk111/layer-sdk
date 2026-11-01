# claw-relay — sovereign relay run (Buzz retired 2026-11-01)

**Status 2026-11-01:** Buzz is retired as the coordination relay.
claw-relay is the authoritative bus; the beside-Buzz conformance-diff
doctrine no longer applies (no second relay to diff against). The
equivalent gates now: **verified on-chain checkpoint anchors (7 days) +
joint divergence test clean + staging-daemon soak 24h** before DNS.
Buzz binaries stay on the ops host as cold rollback for 2 weeks.

Admission source as of commit `1a53e91` / v5 build:
`--attestation-contract juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm`
— relay derives npubs directly from `ListActiveSeats` (cw4 join is
server-side; a seat that loses cw4 weight drops out on next refresh).
The `--cw4-group` + `--member-bindings` path remains as legacy fallback.

**Original purpose (pre-retirement):** run claw-relay + claw-checkpoint
in parallel with the live Buzz relay for ~1 week before DNS cutover.

> Rule from spec v0.2: cutover only after `conformance.sh` reads
> all-MATCH for 7 consecutive days.

## Topology

```
                    writes (buzz-cli daemons, unchanged)
                              │
        ┌─────────────────────┴──────────────────────┐
        ▼                                            ▼
  Buzz (wss://relay.junoclaw.xyz)           claw-relay (ws://:7070)
  authoritative                             shadow, same npub allowlist
        ▲                                            │
        │                             claw-checkpoint (hourly cron)
        │                             anchors ITS window on moultbook
        └───────── conformance.sh diffs the two daily ──┘
```

For v1 the shadow relay does NOT mirror writes automatically — the
vote/exec/status daemons keep writing to Buzz only. claw-relay sees:
(a) whatever operators/probes send it, (b) gossip-pull from Buzz is
not possible (Buzz isn't a claw peer) — so shadow-traffic = probe
writes + any daemon we point at it. The honest conformance surface
today is therefore **read-replay for what each relay was served** and
**write-admission behavior** — measured by `conformance.sh`.

## Deploy (on the Buzz ops host)

```bash
# 1. ship binaries (built via docker/rust:1.98-bullseye mount)
scp dist/claw-relay dist/claw-checkpoint <opshost>:/opt/claw/bin/

# 2. state + config — reuses the same state the cw4 watcher uses
sudo mkdir -p /opt/claw/state /opt/claw/db
sudo cp /opt/buzz/state/members.json /opt/claw/state/member-bindings.json
sudo cp /opt/buzz/state/group_addr   /opt/claw/state/group_addr

# 3. break-glass allowlist: the status-bot npub (posting receipts)
sudo cp /opt/buzz/state/my-npub.hex /opt/claw/state/allowlist.txt

# 4. run the relay — ws://127.0.0.1:7070, API on same host :8082
#    v5: attestation-contract replaces cw4-group + member-bindings
nohup /opt/claw/bin/claw-relay \
  --listen 127.0.0.1:7070 \
  --db /opt/claw/db/relay.redb \
  --channels 2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38,810d3c45-acb5-44f8-9155-8bc94b3a12b3,7b4f3765-8494-4de8-bf7a-b2595da827e0 \
  --attestation-contract juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm \
  --chain-api http://127.0.0.1:8082 \
  --allowlist /opt/claw/state/allowlist.txt \
  >> /opt/claw/relay.log 2>&1 &

# 5. checkpoint daemon — hourly, same cadence as the Python anchor
#    (state files are independent; both daemons can anchor the same
#    window — different relays' views of the same channel)
crontab -l > /tmp/cron && cat >> /tmp/cron <<'EOF'
47 * * * * /opt/claw/bin/claw-checkpoint --once \
  --relay ws://127.0.0.1:7070 \
  --channels "governance:2a6c9ad2-9da8-406c-8aa3-cd2f1e33af38 validator-ops:810d3c45-acb5-44f8-9155-8bc94b3a12b3 agents:7b4f3765-8494-4de8-bf7a-b2595da827e0" \
  --moultbook juno1xh8rfpsnh4ylkpr6yanpy2y9903yq2ugr7ycry59p84r5c5pethqua7wum \
  --author $(sudo cat /opt/buzz/state/my-juno1.txt) \
  --api http://127.0.0.1:8082 \
  --state-dir /opt/claw/state \
  --receipt-key-file /opt/buzz/state/status-bot.sec \
  >> /opt/claw/checkpoint.log 2>&1
EOF
crontab /tmp/cron
```

## Daily conformance

```bash
ops/claw-relay/conformance.sh \
  --buzz wss://relay.junoclaw.xyz --claw ws://127.0.0.1:7070 \
  --channel 810d3c45-acb5-44f8-9155-8bc94b3a12b3 \
  --key-file /opt/buzz/state/status-bot.sec \
  --window-secs 86400
```

Read-id-set diffs are EXPECTED until a mirroring path exists — what
must MATCH from day 1: write-accept/deny behavior on identical inputs
(W1, N1–N5). Log the daily report in `/opt/claw/conformance/`.

## Cutover checklist (post-Buzz gates — revised 2026-11-01)

- [x] `ListActiveSeats` (attestation contract) replaces member-bindings
      file — relay flag flips to `--attestation-contract <addr>`
      **[implemented: `--attestation-contract juno1rhlaj…` — npubs decode
      straight from seat records, cw4 weight enforced server-side, no
      bindings file; precedence over the cw4+bindings path]**
- [ ] Joint divergence test clean (`ops/claw-relay/divergence-test.md`):
      two relays, divergent break-glass sets, member events cross both
      directions, break-glass events stay local, replay dedupes, no
      NOTICE/CLOSED storms
- [ ] claw-checkpoint anchors verified on-chain for 7 days (entry refs
      chain unbroken) — the conformance evidence post-Buzz is the
      anchor chain, not a relay-to-relay diff
- [ ] Point BUZZ_RELAY_URL daemons at claw-relay on a staging host first;
      observe 24h; then DNS flip `relay.junoclaw.xyz → claw-relay`
- [x] Buzz retired from write path (2026-11-01 operator decision);
      binaries remain on the ops host as cold rollback for 2 weeks —
      rollback = relaunch Buzz + DNS, not a maintained standby service
