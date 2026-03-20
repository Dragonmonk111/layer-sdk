# Layer SDK Demo

A walkthrough of the Layer Commonware consensus node — what you can run today and what's coming next.

---

## What's Built (Phase 2)

| Component | Status | Notes |
|-----------|--------|-------|
| `slay3rd` binary | ✓ Built | Commonware simplex consensus engine |
| BLS12-381 threshold signatures | ✓ Working | 2-of-3 threshold, MinSig variant |
| CosmWasm state machine | ✓ Working | `App<T>` processes blocks |
| Block certificate persistence | ✓ Working | `App::set_block_certificate()` per finalized block |
| Key generation tool | ✓ Built | `tools/generate-testnet-keys` |
| BLS certificate verifier | ✓ Built | `tools/verify-cert` |
| Multi-node P2P | ⏳ Phase 3 | `simulated` panics with `BindFailed` on tokio runtime — needs `authenticated` |
| gRPC tx submission | ⏳ Phase 3 | Not yet wired (stub in `main.rs`) |
| Contract upload / execute | ⏳ Phase 3 | Depends on gRPC tx path |

**TL;DR**: The consensus state machine is correct and verified. The networking and transaction submission are wired up in Phase 3.

---

## Prerequisites

```bash
# Rust toolchain (1.75+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# From the repo root
cd /path/to/layer-sdk
```

---

## Part 1 — Build the binary

```bash
cargo build -p slay3rd --release
# → target/release/slay3rd
```

Build the supporting tools:

```bash
cargo build --manifest-path tools/generate-testnet-keys/Cargo.toml --release
cargo build --manifest-path tools/verify-cert/Cargo.toml --release
```

---

## Part 2 — Run the unit tests

The unit tests validate the full consensus state machine locally — genesis, propose, verify, certify, BLS certificate storage:

```bash
# Consensus state machine (determinism, block production, certify flow)
cargo test -p slay3rd

# App layer (certificate round-trip, state machine)
cargo test -p layer-app -- test_set_and_get_block_certificate

# Full workspace
cargo test --workspace
```

Expected output includes:
```
test test_genesis_is_deterministic ... ok
test test_certify_calls_finalize_block ... ok
test test_certify_removes_pending_payload ... ok
test test_sequential_certify_increments_height ... ok
...
```

---

## Part 3 — Generate testnet keys

```bash
cargo run --manifest-path tools/generate-testnet-keys/Cargo.toml -- \
    --output-dir /tmp/layer-demo \
    --validators 3
```

This writes 3 validator key files:
```
/tmp/layer-demo/
├── validator-0/keys.json   # BLS share, Ed25519 identity, peer key list
├── validator-1/keys.json
└── validator-2/keys.json
```

Each `keys.json` contains:
- `bls_private_hex` — BLS12-381 secret share
- `threshold_public_key_hex` — group threshold public key (same for all)
- `ed25519_public_hex` — this node's P2P identity key
- `validator_public_keys` — ordered list of all 3 validators' Ed25519 keys
- `threshold_required: 2`, `threshold_total: 3` — 2-of-3 BFT threshold

---

## Part 4 — Verify a BLS certificate (offline)

The `verify-cert` tool verifies BLS12-381 threshold certificates produced by the consensus engine.

**Presence check** (non-cryptographic — confirms certificate is non-empty):
```bash
# After a run, the certificate hex can be found in node logs:
# grep "Block finalized with BLS threshold certificate" /tmp/layer-testnet/node-0/node.log

echo "CERT_HEX_HERE" > /tmp/demo_cert.hex
cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
    --cert-file /tmp/demo_cert.hex \
    --check-presence
```

**Full cryptographic verification**:
```bash
# Requires: cert hex, threshold public key, and message (payload digest)
cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
    --cert <CERT_HEX> \
    --keys-file /tmp/layer-demo/validator-0/keys.json \
    --message <PAYLOAD_DIGEST_HEX>
```

Exit codes: `0` = valid, `4` = verification failed.

---

## Part 5 — Determinism audit

Confirms there are no `HashMap`, `HashSet`, or `SystemTime::now()` calls in consensus-critical code paths (required for identical `AppHash` across validators):

```bash
bash demo/scripts/determinism-audit.sh
```

---

## Part 6 — Why the node panics (and what Phase 3 fixes)

Running `scripts/testnet.sh start` currently produces a panic:

```
ERROR commonware_runtime::utils::handle: task panicked err="BindFailed"
thread panicked at commonware-p2p-2026.3.0/src/simulated/network.rs:1154:
called `Result::unwrap()` on an `Err` value: BindFailed
```

**Root cause**: `commonware_p2p::simulated` generates a random IPv4 address (via `OsRng.next_u32() → Ipv4Addr::from_bits(random_u32)`) and calls `TcpListener::bind(random_ip)`. On macOS that IP is never assigned to a local interface, so the bind fails with `EADDRNOTAVAIL`.

The simulated network was designed for `commonware_runtime::deterministic` (a fake networking layer used in tests). Phase 2 pairs it with `commonware_runtime::tokio`, which tries to make real TCP sockets — hence the crash.

**Phase 3 fix**: Replace `commonware_p2p::simulated` with `commonware_p2p::authenticated`. That module uses real TCP connections with Ed25519-authenticated channels and is designed specifically for the tokio runtime. Each `slay3rd` node will bind to its configured `p2p_listen` address and connect to peers over the network.

**What this means today**: You can't run a live node in Phase 2. Everything else in this demo (unit tests, keygen, verify-cert, determinism audit) works fine.

---

## Phase 3 Preview — Full Transaction Flow

Once Phase 3 wires real P2P (`commonware_p2p::authenticated`) and gRPC tx submission, this will be the complete flow:

### 1. Start the 3-node testnet
```bash
scripts/testnet.sh start
scripts/testnet.sh wait 10   # wait for block height 10
```

### 2. Upload a CosmWasm contract
```bash
# Build the contract
cd lib/cosmwasm/contracts/hackatom
cargo wasm   # → target/wasm32-unknown-unknown/release/hackatom.wasm

# Upload (MsgStoreCode via gRPC on port 9090)
# grpcurl or layerd CLI — not yet wired
grpcurl -plaintext -d '{
  "sender": "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug",
  "wasm_byte_code": "'$(base64 < target/wasm32-unknown-unknown/release/hackatom.wasm)'"
}' localhost:9090 cosmwasm.wasm.v1.Msg/StoreCode
# → returns code_id: 1
```

### 3. Instantiate the contract
```bash
grpcurl -plaintext -d '{
  "sender": "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug",
  "code_id": 1,
  "label": "demo-hackatom",
  "msg": "'$(echo -n '"{"verifier":"layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug","beneficiary":"layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug"}"' | base64)'"
}' localhost:9090 cosmwasm.wasm.v1.Msg/InstantiateContract
# → returns contract_address
```

### 4. Execute a transaction
```bash
grpcurl -plaintext -d '{
  "sender": "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug",
  "contract": "<CONTRACT_ADDRESS>",
  "msg": "'$(echo -n '"{"release":{}}"' | base64)'"
}' localhost:9090 cosmwasm.wasm.v1.Msg/ExecuteContract
```

### 5. Query state
```bash
grpcurl -plaintext -d '{
  "address": "<CONTRACT_ADDRESS>",
  "query_data": "'$(echo -n '"{"verifier":{}}"' | base64)'"
}' localhost:9090 cosmwasm.wasm.v1.Query/SmartContractState
```

### 6. Verify BLS certificate for the block containing your tx
```bash
# Get cert from node log after tx is included in a block
cert_hex=$(grep "Block finalized with BLS threshold certificate" \
    /tmp/layer-testnet/node-0/node.log | tail -1 | grep -oP 'certificate=\K[0-9a-f]+')

cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
    --cert "$cert_hex" \
    --keys-file /tmp/layer-testnet/validator-0/keys.json \
    --message <PAYLOAD_DIGEST>
```

### 7. Stop the testnet
```bash
scripts/testnet.sh stop
```

---

## Architecture Reference

```
slay3rd binary
├── commonware simplex Engine         ← consensus (BLS 2-of-3)
│   ├── LayerNode (CertifiableAutomaton)
│   │   ├── propose()  → drain Mempool → BlockPayload → digest
│   │   ├── verify()   → check pending_payloads map
│   │   └── certify()  → App::finalize_block() [DETERMINISM CRITICAL]
│   └── LayerReporter
│       └── report(Finalization) → App::set_block_certificate(height, cert)
│
├── App<MemoryStore>                  ← CosmWasm state machine
│   ├── init()         → genesis state
│   ├── finalize_block() → run CosmWasm txs, update app_hash
│   └── set_block_certificate()      ← CONS-05: persists BLS cert
│
└── gRPC server                       ← ⏳ Phase 3
    ├── MsgStoreCode
    ├── MsgInstantiateContract
    ├── MsgExecuteContract
    └── BroadcastTx
```

**Key invariant**: `certify()` is the single, deterministic commit point. All validators execute identical code paths with no `HashMap`, `SystemTime::now()`, or floats — guaranteeing identical `AppHash` across the network.

---

## Config Reference

Node config (`config.toml`):

```toml
validator_index = 0           # 0-based index into validator set
chain_id = "slay3r-testnet-1"
p2p_listen = "127.0.0.1:26656"
grpc_listen = "127.0.0.1:9090"
peers = ["127.0.0.1:26657", "127.0.0.1:26658"]
bls_key_path = "/tmp/layer-demo/validator-0/keys.json"
identity_key_path = "/tmp/layer-demo/validator-0/keys.json"
wal_path = "/tmp/layer-demo/node-0/wal"
genesis_path = "/tmp/layer-demo/genesis.json"
mempool_max_pending = 10000
leader_timeout_ms = 3000
certification_timeout_ms = 5000
```

Genesis (`genesis.json`):

```json
{
  "chain_id": "slay3r-testnet-1",
  "initial_height": 1,
  "app_state": {
    "bank": [],
    "wasm": {
      "gov_account": "layer1pkptre7fdkl6gfrzlesjjvhxhlc3r4gmt53rug"
    }
  }
}
```

Environment variables:
- `RUST_LOG` — log verbosity (`info`, `debug`, `trace`)
- `SLAY_CONFIG` — path to config file (alternative to positional arg)
