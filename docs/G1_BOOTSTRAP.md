# G1 Bootstrap Runbook

*G1 = closed testnet, 3–5 invited external validators. Everything here has
been run end-to-end on devnet or rehearsed with `generate-testnet-keys`.*

## Release

- Tag: `v0.6.0-rc1` on commit `59ba9f3` (node image; the keygen tool changes
  land on `main` right after the tag — operators build the tool from `main`).
- Artifacts in `releases/v0.6.0-rc1/`:
  - `junoclaw-chain-v0.6.0-rc1.tar.gz` — `docker save` of the image (`docker load < file`)
  - `slay3rd` — Linux x86_64 binary (builds/runs without Docker)
  - `SHA256SUMS.txt` — hashes of both, plus commit + image digest.

Every participant verifies:

```bash
sha256sum -c SHA256SUMS.txt
docker load < junoclaw-chain-v0.6.0-rc1.tar.gz
docker image inspect junoclaw-chain:v0.6.0-rc1 --format '{{.Id}}'
# must equal the digest recorded in SHA256SUMS.txt
```

## Key ceremony

Prerequisites: each operator on **Linux** (MAYO2 keygen is Unix-only) with the
`generate-testnet-keys` binary built from this repo:

```bash
cargo build --release --manifest-path tools/generate-testnet-keys/Cargo.toml
```

1. **Each operator** (locally):

   ```bash
   generate-testnet-keys keygen-share --output share-request.json
   ```

   Generates Ed25519 identity + MAYO2 key **locally — they never leave the
   machine** — and emits a `share-request.json` with only public material.
   The operator sends that file to the coordinator (any channel; it is public).

2. **Coordinator**:

   ```bash
   generate-testnet-keys assemble-genesis \
       --input-dir share-requests/ \
       --output-dir ceremony-out/ \
       --chain-id junoclaw-g1
   ```

   Deals the BLS threshold shares from `OsRng`, writes `shared.json`,
   `genesis.json`, and one package per validator: `keys.json` (their BLS
   share), `node-<i>.toml`. A missing `--chain-id` now prints a warning —
   the default `junoclaw-1` reads like a mainnet id.

3. **Coordinator distributes** each `keys.json` over an encrypted channel
   (age, encrypted DM) and **deletes all local copies**. The coordinator
   transiently sees all BLS shares (trusted dealing). A certificate still
   needs a MAYO2 quorum, and those keys never left operator machines, so
   the coordinator alone cannot forge one. A dealer-free DKG is planned
   before mainnet.

4. **Each operator** runs `generate-testnet-keys finalize` (or just places
   `keys.json` + `node.toml` + `genesis.json` and starts the node).

## Node config (what the template emits)

```toml
validator_index = <i>
chain_id = "junoclaw-g1"
p2p_listen = "0.0.0.0:7001"
grpc_listen = "0.0.0.0:9090"
bls_key_path = "/keys/keys.json"
identity_key_path = "/keys/keys.json"
data_dir = "/data"
genesis_path = "/config/genesis.json"
mempool_max_pending = 10000
leader_timeout_ms = 3000
certification_timeout_ms = 5000
pruning = "validator"
# insecure_devnet must stay unset on a real network
hybrid_consensus = true

[[peers]]
public_key = "<peer ed25519>"
address = "<peer-ip>:7001"   # literal IP — DNS names are not resolved
```

Port: **inbound TCP 7001** must be reachable (P2P). gRPC 9090 optional.
Stable public IP required — a small cloud VM or a VPS in front of a home
node works; Cloudflare Tunnel does not (it doesn't carry raw TCP P2P).

## WAN rehearsal (before inviting operators)

Every devnet figure comes from one host — consensus messages on loopback are
~0 RTT. G1 must be rehearsed across real networks: 2 cheap cloud VMs in
different regions + your machine, 3 validators, same ceremony flow.

### Expected block times

Simplex finality = proposer broadcast + 2f+1 vote collection; the wall-clock
cost is dominated by the **quorum** path, not the slowest peer. Measured
baseline on loopback: ~0.16 s.

| Topology | Typical RTT to quorum | Expected block time |
|---|---|---|
| Same region (3× same DC) | 1–5 ms | ~0.2–0.3 s |
| Same continent (e.g. EU + UK + home) | 15–40 ms | ~0.3–0.5 s |
| Transatlantic mix (EU + US-E + US-W) | 80–150 ms | ~0.5–1.0 s |
| Global spread (EU + US + APAC) | 150–250 ms | ~0.8–1.5 s |

Notes:

- With 3–5 validators, `leader_timeout_ms = 3000` still has ~2–10× headroom
  over the global-spread case. Only retune if rehearsal shows certification
  timeouts in logs (`view timeout` WARN lines).
- Block time follows the **quorum** RTT, so one slow validator stretches the
  tail but doesn't gate every block — unless it leads.
- Measure during rehearsal: `docker logs <node> | grep CONS-05` gives
  per-height finality timestamps; diff consecutive heights for block period.

### Rehearsal checklist

1. Run the full ceremony exactly as above (real IPs, real encrypted channel).
2. Boot all 3 nodes; confirm finality advancing, `CONS-05` heights agree.
3. Kill the current leader mid-run → chain must skip its view and continue.
4. Restart a node with its volume intact → must catch up via backfill.
5. Leave it running ≥24 h; record p50/p99 block time, WARN/ERROR counts.
6. Report numbers back into this file before inviting external operators.
