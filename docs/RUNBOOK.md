# Validator Runbook — junoclaw devnet

Incident response distilled from live soak events (2026-10-02). Every
procedure here has been executed against the running devnet.

Conventions: nodes are containers `junoclaw-node-0..3` on docker network
`devnet_junoclaw-devnet`. Compose pins `node-N` to `172.28.0.(10+N)`;
every peer `[[peers]]` table and every `[state_sync]` peer list assumes
those addresses. All docker commands run from the repo root.

## Node liveness check (30 seconds)

```powershell
docker ps --format "{{.Names}} {{.Status}}"
foreach ($n in 0..3) {
  docker logs "junoclaw-node-$n" --tail 60 2>&1 |
    Select-String "Block finalized" | Select-Object -Last 1
}
```

Healthy = each node logs `Block finalized` within the last ~2 s of wall
time and the heights agree within ±2.

## Lag tiers — pick the recovery by gap size

Get the victim's height and the tip, subtract.

### Tier 0 — process dead (container exited)

```powershell
docker inspect junoclaw-node-N -f "{{.State.Status}} {{.State.ExitCode}}"
docker start junoclaw-node-N
```

If it crash-loops, read `docker logs --tail 200` for the fatal line before
restarting again.

### Tier 1 — small gap (< ~500 blocks)

Just let backfill work. Symptom: `finalized payload unavailable` errors
interleaved with `Payload relay: received` inserts; height climbs within
minutes. No action needed.

### Tier 2 — medium gap (~500–5k)

Backfill still works but check it is actually progressing — count inserts:

```powershell
docker logs junoclaw-node-N --since 60s 2>&1 |
  Select-String "inserted into pending_payloads" | Measure-Object
```

> 0 = recovering, leave it. 0 = stalled (see Tier 3).

### Tier 3 — large gap (> ~5k) OR stalled backfill

**Do not wait for backfill.** Deep catch-up via per-height fetch is slow
and (pre-fix image) can stall outright. State-sync is the correct path —
verified live: wipe → certified snapshot → tip in seconds.

```powershell
docker stop junoclaw-node-N
docker rm junoclaw-node-N
docker volume rm devnet_nodeN_data
docker compose -f devnet/docker-compose.yml up -d node-N
```

Confirm adoption in the first log lines (`snapshot` / `state` /
`anchor` keywords), then watch it reach tip. Requires
`[state_sync]` peers configured in `devnet/config/node-N.toml` and
`min_anchor_agree` (default 2) donors live.

## The silent wedge — container up, zero activity

Symptom: `docker ps` healthy, CPU idle, logs frozen at an old timestamp,
monitor reports DOWN.

**First check the IP.** This exact failure was hit live: a
`docker network disconnect/connect` cycle reassigned the container's IP,
so peers kept dialing the pinned address of a container that no longer
lived there.

```powershell
docker inspect junoclaw-node-N | Select-String '"IPAddress"'
```

Expected: `172.28.0.(10+N)`. If it differs, restore it:

```powershell
# compose pins node-N to 172.28.0.(10+N): node-0→.10 … node-3→.13
$victim = N
$pinIP = "172.28.0.$($victim + 10)"
docker network disconnect devnet_junoclaw-devnet junoclaw-node-$victim
docker network connect --ip $pinIP devnet_junoclaw-devnet junoclaw-node-$victim
docker restart junoclaw-node-$victim
```

Relays resume within seconds if this was the cause. **Never run a bare
`docker network connect` on a validator container** — always pass `--ip`
with the compose-pinned address.

If the IP is already correct, fall through to a plain restart, then
Tier 3 state-sync if the gap is large.

## Divergence — the one alert that is NOT cosmetic

Two different hashes appear in logs. Know the difference cold:

- **`app_hash`** = `FastHasher` rolling write-history hash. A
  state-synced node legitimately reports a *different* app_hash than
  replay nodes forever — same state, different write history. **A
  mismatch here is expected and harmless.** The monitor compares
  `digest` (certified payload hash), not this.
- **`state_root`** = Merkle root over consensus state, bound into every
  certified `BlockPayload`. A mismatch halts the node (fail-stop by
  design) — the log line is `state_root mismatch`. **This is the real
  emergency.** The node refuses to execute a divergent chain.

If a `state_root mismatch` halt fires: do NOT restart-loop it. Capture
`docker logs junoclaw-node-N --tail 500` to a file, note the height, and
compare `_payload/` digests at that height across nodes before any
recovery. The halt is the safety property working — investigate *why*
before clearing it.

## Chaos-soak operations (soak-c9.ps1)

Events fire every ~60–120 min: kill+restart, 90–180 s partition, compose
recreate, byzantine-proposer. Log: `soak-c9.log` (EVENT lines + heartbeat
tip every 5 min).

- **Partition leg MUST re-pin the IP** — already patched; if editing the
  leg keep `docker network connect --ip 172.28.0.1N`.
- **Byzantine leg** needs an image built with `fault_inject` support.
  On an older image the toml key is ignored and the leg degrades to a
  plain restart — harmless but tests nothing.
- **Never rebuild `junoclaw-chain:latest` while a soak is running** — a
  recreate event would silently boot a mixed-version validator and poison
  the run's results.
- After recovery from a partition, confirm `broadcast to 3 peers` in the
  victim's logs before counting the event recovered.

## Monitoring (monitor-s4.ps1)

Polls all four logs every 30 s → `monitor-s4.log`. Alert conditions:
certified-payload **digest** mismatch, `state_root mismatch` halt lines,
lag > 10 blocks, stall > 90 s. `DOWN` for a node that produces no
`certificate stored` line in the window — usually means it is
backfilling (check insert counts) rather than dead.
