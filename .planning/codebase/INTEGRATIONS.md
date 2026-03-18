# External Integrations

**Analysis Date:** 2026-03-18

## APIs & External Services

**Blockchain RPC:**
- Tendermint RPC endpoint
  - Client: `tendermint-rpc` (0.39.1)
  - Configuration: `rpc_url` environment variable
  - Used by: Transaction submission, blockchain state queries
  - Default: `http://localhost:26657`
  - Accessed in: `app/slay3rd/src/grpc/tx.rs`, `app/slay3rd/src/grpc/tendermint.rs`

**Cosmos gRPC:**
- CometBFT node gRPC endpoint (port 9090)
  - Client: `tonic` (0.12.2)
  - Server binding: Configured via `grpc` parameter
  - Default: `0.0.0.0:9090`
  - Serves: Auth queries, bank queries, CosmWasm queries, sync info, tendermint queries, tx service

## Data Storage

**Databases:**
- RocksDB (optional, v0.22.0)
  - Connection: File path via `rocksdb` config parameter
  - Storage abstraction: `layer-storage` package at `packages/storage/Cargo.toml`
  - Feature flag: `rocksdb` (enabled in `app/slay3rd/Cargo.toml`)
  - Alternative: In-memory storage via `MemoryStore` (used when `rocksdb` not configured)
  - Used for: Blockchain state persistence

**Smart Contract State:**
- CosmWasm storage
  - Framework: `cw-storage-plus` (1.2.0)
  - VM: `cosmwasm-vm` (1.5.4)
  - Location: WASM files stored in `$SLAY_HOME/data/` directory
  - Configured in: `app/slay3rd/src/main.rs` (line 114)

**File Storage:**
- Local filesystem only
  - Home directory: `$SLAY_HOME` (default: `$HOME/.slay3r`)
  - Data directory: `$SLAY_HOME/data/` for WASM contracts
  - Config directory: `$SLAY_HOME/config/`

**Caching:**
- None - Direct storage access via RocksDB or memory store

## Authentication & Identity

**Auth Provider:**
- Custom - Cosmos SDK Auth module
  - Implementation: `cosmos/auth/v1beta1` protobuf definitions
  - Query endpoint: `/cosmos.auth.v1beta1.Query`
  - Supports: Account queries, pubkey types (Ed25519, Secp256k1)
  - Exposed via: gRPC at `app/slay3rd/src/grpc/auth.rs`

**Cryptographic Keys:**
- Ed25519 - Primary key type
  - Client: `cosmos/crypto/ed25519` protobuf
- Secp256k1 - ECDSA key type
  - Client: `cosmos/crypto/secp256k1` protobuf
- Key generation via CosmJS clients

**Client Integration:**
- CosmJS - Cosmos blockchain client library
  - Packages: `@cosmjs/stargate`, `@cosmjs/amino`, `@cosmjs/cosmwasm-stargate`
  - Integration tests: `js/` directory
  - Used for: Transaction signing, account queries, contract interactions

## Monitoring & Observability

**Tracing & APM:**
- Jaeger - Optional distributed tracing
  - Export method: OpenTelemetry collector client via HTTP
  - Configuration: `jaeger` parameter (endpoint URL)
  - Format: `http://[host]:[port]/api/traces`
  - Service name: `slay3rd`
  - Default endpoint: `http://localhost:14268/api/traces` (local Jaeger)
  - Timeout: 2 seconds
  - Runtime: Tokio async
  - Configuration: `app/slay3rd/src/main.rs` (lines 78-91)

**Logs:**
- Structured logging via Tracing
  - Framework: `tracing` (0.1.37) + `tracing-subscriber` (0.3.17)
  - Format: RFC 3339 timestamps with local timezone
  - Levels: Configurable via `RUST_LOG` or config
  - Output: Stdout with ANSI colors (development)
  - Integration: OpenTelemetry layers for Jaeger export

**Container Observability:**
- Jaeger UI - Web interface on port 8080
  - Access: `http://localhost:8080`
  - Image: `jaegertracing/all-in-one:1.59`
  - Ports: 6831/udp, 6832/udp (agent), 14268 (collector), 16686 (UI)

## CI/CD & Deployment

**Hosting:**
- Docker containers (primary)
  - Images: `ghcr.io/lay3rlabs/slay3rd:0.5.0`, `ghcr.io/lay3rlabs/gateway:0.5.0`
  - Registry: GitHub Container Registry (ghcr.io)
- Bare metal - Optional (Ubuntu 22.04 recommended)

**Deployment Platform:**
- Docker Compose (local and production)
- File: `docker-compose.yml` (root directory)
- Volumes: Named volumes for data persistence
  - `lay3r_data` - Slay3r blockchain state
  - `comet_data` - CometBFT state
  - `wasmatic_data` - Faucet data (if used)

**CI Pipeline:**
- GitHub Actions (inferred from Docker image registry)
- Build on: Tag or commit to main branch
- Output: Multi-arch Docker images pushed to ghcr.io

## Environment Configuration

**Required env vars:**
- `SLAY_HOME` - Home directory for config/data (optional, defaults to `$HOME/.slay3r`)
- `SLAY_` prefix - Environment variable prefix for all config parameters
- `RUST_BACKTRACE` - Error backtrace level (set to 1 in docker-compose)

**Optional env vars:**
- `RUST_LOG` - Logging level and filter
- `CMT_LOG_LEVEL` - CometBFT logging configuration

**Secrets location:**
- No .env file detected - Configuration via TOML + environment variables
- Jaeger credentials optional (username/password fields commented in code)

## Service Ports

**Internal Services:**
- CometBFT RPC: `localhost:26657`
- CometBFT P2P: `localhost:26656`
- Slay3r ABCI: `localhost:26658`
- Slay3r gRPC: `localhost:9090`

**Public APIs:**
- REST API (Gateway): `0.0.0.0:1317` (default)
- gRPC API: `0.0.0.0:9090`

**Observability Ports:**
- Jaeger UI: `0.0.0.0:8080`
- Jaeger Agent: UDP 6831, 6832
- Jaeger Collector: `0.0.0.0:14268`

## Webhooks & Callbacks

**Incoming:**
- None detected

**Outgoing:**
- Jaeger tracing export
  - Target: Configured Jaeger collector endpoint
  - Protocol: gRPC/HTTP via OpenTelemetry
  - Async: Batched with Tokio runtime

**Blockchain Callbacks:**
- ABCI callbacks from CometBFT to Slay3r
  - Channel: TCP on `localhost:26658`
  - Protocol: Tendermint ABCI
  - Bidirectional: Consensus → App state machine

## Service Dependencies

**Service Graph:**
```
CometBFT (Consensus)
    ↓ (ABCI on 26658)
Slay3r App (ABCI + gRPC)
    ↓ (gRPC on 9090)
Gateway (REST API on 1317)

Slay3r → Jaeger (Tracing)
Slay3r → RocksDB (Persistence)
```

**Docker Compose Services:**
- `slay3r` - Application server (depends on: jaeger)
- `cometbft` - Consensus engine (depends on: slay3r)
- `gateway` - REST gateway (depends on: slay3r)
- `jaeger` - Tracing backend (standalone)
- `faucet` - Token faucet (optional)

---

*Integration audit: 2026-03-18*
