# JunoClaw Validator Onboarding

How to run a `slay3rd` validator node on the JunoClaw devnet (and eventually mainnet).

JunoClaw consensus is **Commonware simplex** with **BLS12-381 threshold signatures** —
not Tendermint, not stake-weighted voting. The validator set is fixed at genesis.
Every validator has equal weight; finality requires a threshold of BLS shares.

---

## 1. Requirements

### Hardware (devnet)

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU      | 2 cores | 4 cores     |
| RAM      | 4 GB    | 8 GB        |
| Disk     | 20 GB SSD | 50 GB SSD |
| Network  | 10 Mbps, stable | 100 Mbps, static IP |

### Software

- Linux (Ubuntu 22.04+ recommended) or Docker
- Rust 1.85+ (if building from source)
- Open ports: P2P (default `7001`), gRPC (default `9090`, optional public)

### Keys

Each validator needs **two keys**, both stored in a single `keys.json`:

- **BLS12-381 share** — your threshold signing share for consensus votes/certificates
- **Ed25519 identity key** — authenticates your P2P connection to peers

> **Devnet:** keys are generated centrally by the coordinator via
> `tools/generate-testnet-keys` and distributed to each validator.
>
> **Mainnet:** keys will be generated via a distributed DKG ceremony —
> no single party ever holds the full key. See `VALIDATOR_SET_DKG_PLAN.md`.

---

## 2. Get the Binary

### Option A — Build from source

```bash
git clone <junoclaw-chain-repo>
cd junoclaw-chain
cargo build --release -p slay3rd
# binary: target/release/slay3rd
```

### Option B — Docker

```bash
docker build -f docker/Dockerfile.slay3rd -t junoclaw/slay3rd .
```

---

## 3. Key Generation (devnet)

The coordinator runs:

```bash
cargo run --manifest-path tools/generate-testnet-keys/Cargo.toml -- \
    --output-dir ./testnet-keys \
    --validators 3
```

This produces:

```
testnet-keys/
├── validator-0/keys.json     # BLS share + Ed25519 identity for validator 0
├── validator-1/keys.json
├── validator-2/keys.json
└── shared.json               # threshold_public_key_hex + validator_public_keys
```

You receive **your** `validator-<i>/keys.json` plus `shared.json` (or the
equivalent values baked into genesis). Never share `keys.json` — it contains
your secret BLS share and Ed25519 private key.

---

## 4. Configure Your Node

Create `node.toml` (see `devnet/config/node-0.toml` for a working example):

```toml
# Your index in the static validator set (0-based). Assigned by coordinator.
validator_index = 0
chain_id = "junoclaw-1"

# Network
p2p_listen  = "0.0.0.0:7001"
grpc_listen = "0.0.0.0:9090"

# Key material (paths inside container or on host)
bls_key_path      = "/keys/validator-0/keys.json"
identity_key_path = "/keys/validator-0/keys.json"

data_dir     = "/data"
genesis_path = ""               # set when mainnet genesis exists

# Consensus tuning (defaults are fine for devnet)
mempool_max_pending        = 10000
leader_timeout_ms          = 3000
certification_timeout_ms   = 5000

# Peers — every other validator's Ed25519 pubkey + address
[[peers]]
public_key = "<ed25519-pubkey-hex>"
address    = "<host>:7001"
```

**Rules:**

- `validator_index` must match the index your `keys.json` was generated for.
- `[[peers]]` lists the *other* validators — your own entry is optional/ignored.
- `chain_id` must be identical across the set.

---

## 5. Run

### Docker Compose (devnet reference)

```bash
cd devnet
docker compose up -d
docker compose logs -f node-0
```

The devnet compose file mounts `devnet/config/node-<i>.toml` and
`devnet/keys/validator-<i>/` into each container.

### Bare metal / single node

```bash
slay3rd --config /path/to/node.toml
```

---

## 6. Verify You're Participating

Healthy signs in logs:

- `simplex` leader proposals at each height
- BLS partial signatures being broadcast and aggregated
- Block certificates finalizing (threshold reached)
- gRPC queries returning increasing heights:

```bash
grpcurl -plaintext localhost:9090 cosmos.bank.v1beta1.Query/TotalSupply
```

If your node is up but not signing, check:

1. `validator_index` matches your key share index
2. Peers are reachable on their P2P ports
3. `chain_id` matches the rest of the set
4. Clock sync (NTP) — simplex timeouts are wall-clock sensitive

---

## 7. Operations

### Upgrades

The validator set is static — there is no in-protocol add/remove yet.
Set changes ship as a **coordinated config + binary upgrade**: all validators
swap `node.toml` (new peer list / indices) and restart at an agreed height.
See `VALIDATOR_SET_DKG_PLAN.md` for the epoch-based roadmap.

### Slashing

Consensus validators are **not slashed** — there is no staking in consensus.
Equivocation safety comes from the BLS threshold itself: a double-signing
validator can only corrupt its own share, not forge a certificate.

The **truth market** (CosmWasm layer) is where economic stake lives:
operators bond `ujclaw`, submit verdicts, and get slashed for divergence.
That is a separate role from running a consensus validator.

### Backups

- `keys.json` — back up once, store offline. Losing it means losing your share.
- `data_dir` — can be rebuilt by re-syncing; not critical to back up.

### Monitoring

- gRPC health endpoint on `grpc_listen`
- Log lines for certificate finalization per height
- (Roadmap) Prometheus metrics exporter

---

## 8. Mainnet Checklist (when announced)

- [ ] Participate in DKG ceremony (generate your share locally — never send it)
- [ ] Receive `shared.json` / genesis with threshold pubkey + peer set
- [ ] Configure `node.toml` with assigned `validator_index`
- [ ] Open P2P port, verify connectivity to all peers before genesis time
- [ ] Start node, confirm first certificate finalizes

Questions → JunoClaw validator channel on the buzz relay (`wss://buzz.junoclaw.xyz`).
