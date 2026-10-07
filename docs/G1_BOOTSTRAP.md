# G1 Bootstrap Runbook

*G1 = closed testnet, **4 invited validators** (N3f1 → n=3f+1, f=1 —
`assemble-genesis` hard-refuses fewer than 4 share-requests). Everything
here has been run end-to-end on devnet or rehearsed with
`generate-testnet-keys`.*

*Launch plan: the G1 launch **is** the WAN rehearsal — one ceremony across
real geography (OVH + Akash + operator machines), 24–72 h soak, publish the
numbers. There is no separate private rehearsal.*

## Release

- Tag: `v0.6.0-rc1` on commit `59ba9f3` (node image; the keygen tool changes
  land on `main` right after the tag — operators build the tool from `main`).
- Artifacts in `releases/v0.6.0-rc1/`:
  - `junoclaw-chain-v0.6.0-rc1.tar.gz` — `docker save` of the image (`docker load < file`)
  - `slay3rd` — Linux x86_64 binary (builds/runs without Docker)
  - `generate-testnet-keys-linux-x86_64` — ceremony tool, statically linked Linux build
  - `SHA256SUMS.txt` — hashes of all of the above, plus commit + image digest.

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

1. **Each operator** (locally, on the machine that will run the node):

   ```bash
   generate-testnet-keys keygen-share \
       --output-dir ./my-validator --name <moniker> --p2p <public-ip>:7001
   ```

   Generates Ed25519 identity + MAYO2 key **locally — they never leave the
   machine** — into `./my-validator/keys.json` (private), and emits
   `./my-validator/share-request.json` with only public material +
   `--p2p` address. The operator sends `share-request.json` to the
   coordinator (any channel; it is public).

   **Akash seat caveat:** Akash assigns the external `host:port` only
   *after* the lease deploys. Deploy the container first (it can idle),
   note the assigned endpoint, then run `keygen-share --p2p <assigned-endpoint>`
   and send the share-request. The endpoint stays fixed for the lease's
   lifetime.

2. **Coordinator**:

   ```bash
   generate-testnet-keys assemble-genesis \
       --input-dir share-requests/ \
       --output-dir ceremony-out/ \
       --chain-id junoclaw-g1
   ```

   Deals the BLS threshold shares from `OsRng`, writes `shared.json`
   (public ceremony record), and one `validator-<i>/` package per seat:
   `bls-share.json` (their private share) + `node-<i>.toml` (config
   template with peers already wired). A missing `--chain-id` now prints
   a warning — the default `junoclaw-1` reads like a mainnet id.

   Separately, build the chain's `genesis.json` (app state — balances,
   wasm params, gov account):

   ```bash
   node snapshot/build-genesis.mjs \
       --snapshot snapshot/juno-1-snapshot-41655555.json \
       --output genesis.json \
       --dao <juno1...key-controlled-account> \
       [--treasury <juno1...>] [--gov <juno1...>]
   ```

   `--dao` must be a **key-controlled** `juno1` account (the ceremony-
   operated G1 key — generate a fresh secp256k1 key for it, *not* the
   public devnet deployer). Ship the same `genesis.json` to every
   validator; the node refuses genesis files that fund the built-in
   devnet addresses unless `insecure_devnet` is set.

3. **Coordinator distributes** each `validator-<i>/bls-share.json` +
   `node-<i>.toml` + `genesis.json` to its owner over an encrypted
   channel (age, encrypted DM) and **deletes all local share copies**.
   The coordinator transiently sees all BLS shares (trusted dealing).
   A certificate still needs a MAYO2 quorum, and those keys never left
   operator machines, so the coordinator alone cannot forge one. A
   dealer-free DKG is planned before mainnet.

4. **Each operator** merges their share into their private `keys.json`:

   ```bash
   generate-testnet-keys finalize \
       --keys ./my-validator/keys.json \
       --bls-share ./bls-share.json   # from coordinator
   ```

   `finalize` verifies the share matches the polynomial and that the
   ceremony's MAYO table contains the operator's own key, then writes
   the complete `keys.json`. Then mount `keys.json` at `/keys/keys.json`,
   `genesis.json` at `/config/genesis.json`, start the node.

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

## Launch = WAN rehearsal

Every devnet figure comes from one host — consensus messages on loopback are
~0 RTT, so G1's first boot doubles as the first real-network measurement.
The 4 seats span real geography (OVH + Akash + operator machines); the
first 24–72 h of uptime **is** the soak. Numbers go into this file (and
release notes) from live operation, not a separate rehearsal net.

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
2. Boot all 4 nodes; confirm finality advancing, `CONS-05` heights agree.
3. Kill the current leader mid-run → chain must skip its view and continue
   (with n=4, quorum=3: one seat down still finalizes).
4. Restart a node with its volume intact → must catch up via backfill.
5. Leave it running ≥24–72 h; record p50/p99 block time, WARN/ERROR counts.
6. Publish numbers — they are the G1 announcement stats.
