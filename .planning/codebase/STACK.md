# Technology Stack

**Analysis Date:** 2026-03-18

## Languages

**Primary:**
- Rust (1.73+) - Blockchain ABCI application, smart contract VM, core packages
- TypeScript (5.0.4) - Integration testing and JavaScript SDK
- Go (1.19) - gRPC REST gateway

**Secondary:**
- Protocol Buffers - Service definitions and message serialization

## Runtime

**Environment:**
- Rust (latest stable) - App server, contracts, packages
- Node.js - JavaScript integration tests
- Go - Gateway REST API
- CometBFT (v0.38.12) - Tendermint-based consensus engine
- Docker - Container orchestration and deployment

**Package Managers:**
- Cargo - Rust package management
  - Lockfile: `Cargo.lock` present
- npm - JavaScript package management
  - Lockfile: `package-lock.json` (implied in workspace)

## Frameworks

**Core Blockchain:**
- Tendermint ABCI (0.39.1) - Consensus interface
- CometBFT (0.38.12) - Consensus engine
- CosmWasm (1.5.4) - Smart contract platform
- Cosmos SDK Proto (0.18.0) - Standard blockchain types

**RPC & Communication:**
- Tonic (0.12.2) - gRPC server framework
- gRPC Gateway (v2.14.0) - REST-to-gRPC proxy
- Tonic Web (0.12.2) - gRPC Web protocol support
- Tendermint RPC (0.39.1) - RPC client

**Testing:**
- Jasmine (5.0.0) - JavaScript test framework
- ts-node (10.9.1) - TypeScript test runner

**Build/Dev:**
- TypeScript (5.0.4) - Type checking
- ESLint (8.41.0) - Linting
- Prettier (2.8.8) - Code formatting

## Key Dependencies

**Critical Blockchain:**
- `cosmwasm-std` (1.5.4) - Smart contract standard library
- `cosmwasm-vm` (1.5.4) - WebAssembly VM for contracts
- `cw-storage-plus` (1.2.0) - Contract storage layer
- `cw20` (1.0) - Token contract standard
- `cosmrs` (0.13.0) - Cosmos transaction builder
- `tendermint-proto` (0.39.1) - Tendermint protobuf types

**Storage:**
- `rocksdb` (0.22.0) - Optional embedded key-value store
  - Configured via feature flag `rocksdb`
  - Alternative: In-memory storage via `MemoryStore`
  - Accessed through `layer-storage` package at `packages/storage/`

**Async Runtime:**
- `tokio` (1.28.0) - Async runtime with multi-threaded support
- `futures` (0.3.30) - Async utilities
- `tokio-stream` (0.1.15) - Stream utilities
- `tokio-rayon` (2.1.0) - Parallel processing support

**Serialization:**
- `prost` (0.13) - Protocol buffer code generation
- `serde` (1.0.160) - Serialization framework
- `serde_json` (1.0.116) - JSON serialization

**Cryptography:**
- `sha2` (0.10.6) - SHA-2 hashing
- `ripemd` (0.1.3) - RIPEMD hashing
- `hex` (0.4.3) - Hexadecimal encoding

**Observability:**
- `tracing` (0.1.37) - Structured logging
- `opentelemetry` (0.19.0) - Observability API
- `opentelemetry-jaeger` (0.18.0) - Jaeger exporter
- `tracing-opentelemetry` (0.19.0) - OpenTelemetry layer
- `tracing-subscriber` (0.3.17) - Logging subscriber

**HTTP/CORS:**
- `tower-http` (0.5) - HTTP middleware with CORS support
- `tower` (0.4.13) - Service composition framework
- `http` (1.0) - HTTP types

**Utilities:**
- `thiserror` (1.0.38) - Error type derive macros
- `anyhow` (1.0) - Flexible error handling
- `itertools` (0.11.0) - Iterator utilities
- `parking_lot` (0.12.1) - Synchronization primitives
- `bytes` (1.4.0) - Byte buffer utilities

**JavaScript/Node:**
- `@cosmjs/stargate` (0.32.4) - Cosmos blockchain client
- `@cosmjs/cosmwasm-stargate` (0.32.4) - CosmWasm client
- `@cosmjs/amino` (0.32.4) - Amino encoding
- `@cosmjs/tendermint-rpc` (0.32.4) - Tendermint RPC client
- `@cosmjs/faucet-client` (0.32.4) - Faucet interaction
- `cosmjs-types` (0.9.0) - CosmJS protocol types

**Go Gateway:**
- `google.golang.org/grpc` (1.50.1) - gRPC framework
- `google.golang.org/protobuf` (1.28.1) - Protocol buffers
- `github.com/grpc-ecosystem/grpc-gateway/v2` (2.14.0) - REST gateway
- `github.com/rs/cors` (1.11.0) - CORS middleware
- `github.com/golang/glog` (1.0.0) - Logging

## Configuration

**Environment:**
- Configuration via `Figment` crate supporting:
  - TOML files (e.g., `~/.slay3r/config/slay3r.toml`)
  - Environment variables (prefixed with `SLAY_`)
  - CLI arguments
- Config file location: `$SLAY_HOME/config/slay3r.toml` or `$HOME/.slay3r/config/slay3r.toml`
- Key configs:
  - `rocksdb` - Path to RocksDB data directory (optional, uses in-memory if not set)
  - `server_port` - Server port for ABCI (default: 26658)
  - `grpc` - gRPC server address binding
  - `rpc_url` - RPC endpoint URL
  - `jaeger` - Jaeger collector endpoint (optional)
  - `read_buf_size` - Buffer size for server reads

**Build:**
- `Cargo.toml` - Rust workspace configuration
- `Rocket.toml` - LCD server configuration (port 1317)
- `go.mod` - Go gateway module file
- Workspace edition: 2021
- Version: 0.5.0

**Docker:**
- `docker-compose.yml` - Multi-service orchestration
- Dockerfiles: `docker/Dockerfile.slay3rd`, `docker/Dockerfile.gateway`, `docker/Dockerfile.faucet`

## Platform Requirements

**Development:**
- Rust 1.73+ (MSRV)
- Node.js (for JavaScript integration tests)
- Go 1.19+ (for gateway)
- Docker & Docker Compose (for local development)
- 4+ GB RAM for build/test
- Unix-like OS (Linux, macOS)

**Production:**
- Docker container deployment
- Ubuntu 22.04 recommended (Hetzner CPX31: 4 vCPU, 8 GB RAM, 160 GB SSD)
- CometBFT consensus engine
- Jaeger (optional, for tracing)

---

*Stack analysis: 2026-03-18*
