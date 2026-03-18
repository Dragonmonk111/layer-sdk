# Codebase Structure

**Analysis Date:** 2026-03-18

## Directory Layout

```
layer-sdk/
├── app/                           # ABCI application implementations
│   └── slay3rd/                   # Main Slay3r blockchain daemon
│       ├── src/
│       │   ├── main.rs            # Entry point, server setup
│       │   ├── app.rs             # Pulsarium ABCI implementation
│       │   ├── cli.rs             # CLI argument parsing
│       │   ├── config.rs          # Configuration handling
│       │   ├── convert.rs         # Type conversions
│       │   ├── encode.rs          # ABCI request encoding
│       │   ├── decode.rs          # ABCI response decoding
│       │   └── grpc/              # gRPC service handlers
│       │       ├── mod.rs
│       │       ├── auth.rs
│       │       ├── bank.rs
│       │       ├── tx.rs
│       │       ├── cosmwasm.rs
│       │       ├── tendermint.rs
│       │       ├── sync.rs
│       │       └── log.rs
│       └── Cargo.toml
├── packages/                      # Core library packages
│   ├── abci/                      # ABCI server and protocol
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── application.rs     # Application trait
│   │       ├── server.rs          # ABCI server
│   │       ├── codec.rs           # Proto codec
│   │       └── error.rs
│   ├── app/                       # Core state machine
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs             # App state and methods
│   │       ├── sm.rs              # StateMachine logic
│   │       ├── sync.rs            # State sync provider
│   │       ├── genesis.rs         # Genesis state
│   │       ├── error.rs           # PulsarError types
│   │       ├── auth/              # Authentication module
│   │       │   ├── mod.rs
│   │       │   ├── keeper.rs      # Auth logic
│   │       │   └── error.rs
│   │       ├── bank/              # Bank module
│   │       │   ├── mod.rs
│   │       │   └── keeper.rs      # Balance tracking
│   │       ├── wasm/              # CosmWasm module
│   │       │   ├── mod.rs
│   │       │   ├── keeper.rs      # Contract state
│   │       │   ├── events.rs
│   │       │   ├── utils.rs
│   │       │   └── vm/            # Virtual machine
│   │       └── testing/           # Test utilities
│   ├── storage/                   # Storage abstraction
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs          # Storage trait definitions
│   │       ├── memory/            # In-memory store
│   │       │   └── mod.rs
│   │       ├── rocks/             # RocksDB store (feature-gated)
│   │       │   └── mod.rs
│   │       ├── prefixed_storage/  # Storage namespacing
│   │       │   └── mod.rs
│   │       ├── wrap/              # Atomic transactions
│   │       │   └── mod.rs
│   │       ├── plus/              # Storage helpers
│   │       │   └── mod.rs
│   │       ├── prices.rs          # Gas pricing
│   │       └── fast_hash.rs       # Hashing
│   ├── proto/                     # Protobuf definitions (compiled)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── protos/            # Generated proto files
│   │       └── extensions/        # Custom proto logic
│   ├── std/                       # Standard types
│   │   └── src/
│   │       ├── lib.rs
│   │       └── api/               # Core message/query types
│   ├── cosmos/                    # Cosmos SDK compatibility
│   │   └── src/
│   │       └── lib.rs
│   └── golem/                     # Utility functions
│       └── src/
│           └── lib.rs
├── contracts/                     # Test smart contracts
│   ├── root/                      # Root contract (core/privileged)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── interface.rs
│   │   │   └── error.rs
│   │   └── Cargo.toml
│   ├── echo/                      # Echo test contract
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── contract.rs
│   │   │   └── msg.rs
│   │   └── Cargo.toml
│   └── caller/                    # Caller test contract
│       ├── src/
│       │   ├── lib.rs
│       │   ├── contract.rs
│       │   └── msg.rs
│       └── Cargo.toml
├── gateway/                       # REST API gateway (Go)
│   ├── main.go                    # Entry point
│   ├── cosmos/                    # Generated proto handlers
│   ├── cosmwasm/
│   ├── tendermint/
│   ├── layer/
│   ├── google/
│   ├── OpenAPI/
│   ├── go.mod
│   └── go.sum
├── proto/                         # Protobuf source files
│   ├── cosmos/
│   ├── cosmwasm/
│   ├── google/
│   ├── layer/
│   └── tendermint/
├── js/                            # TypeScript SDK and tests
│   ├── src/
│   │   ├── abstract.spec.ts
│   │   ├── bank_send.spec.ts
│   │   ├── cw20.spec.ts
│   │   ├── testutils.spec.ts
│   │   ├── utils.ts
│   │   ├── tap-faucet.ts
│   │   └── generate-mnemonic.ts
│   ├── package.json
│   ├── tsconfig.json
│   └── testdata/
├── localnode/                     # Docker Compose and local setup
│   ├── docker-compose.yml
│   ├── abci/                      # ABCI service config
│   ├── comet/                     # CometBFT config
│   └── wasmatic/                  # WASM service config
├── docker/                        # Dockerfiles
│   ├── gateway/
│   ├── slay3rd/
│   └── faucet/
├── scripts/                       # Build and deployment scripts
│   ├── build_proto_gateway
│   └── [other build scripts]
├── docs/                          # Documentation
│   ├── streaming/
│   │   ├── STATE_STREAMING.md
│   │   └── TX_STREAMING.md
│   └── README.md
├── tools/                         # Development tools
│   └── proto-compiler/            # Custom proto compilation
├── Cargo.toml                     # Workspace configuration
├── Cargo.lock
├── README.md
├── DEV_MAP.md
├── DEPLOYMENT.md
└── [config files]
```

## Directory Purposes

**app/slay3rd/:**
- Purpose: Main blockchain application entry point and CLI daemon
- Contains: ABCI app wrapper, gRPC services, configuration, encoding/decoding
- Key files: `main.rs` (entry), `app.rs` (ABCI impl), `grpc/` (API handlers)

**packages/abci/:**
- Purpose: ABCI protocol implementation, server socket handling
- Contains: Trait definitions, multi-threaded dispatcher, codec for protobuf
- Key files: `lib.rs` (exports), `server.rs` (listen), `application.rs` (trait)

**packages/app/:**
- Purpose: Core state machine and module logic
- Contains: App state wrapper, StateMachine, Auth/Bank/Wasm modules
- Key files: `app.rs` (main struct), `sm.rs` (logic), `{auth,bank,wasm}/keeper.rs`

**packages/storage/:**
- Purpose: Storage abstraction and implementations
- Contains: Trait definitions, MemoryStore, RockStore, atomic wrappers
- Key files: `traits.rs` (trait defs), `memory/mod.rs`, `rocks/mod.rs`

**packages/proto/:**
- Purpose: Compiled protobuf definitions from `proto/`
- Contains: Generated Rust code, proto extensions
- Key files: `lib.rs` (re-exports), `protos/` (generated)

**packages/std/:**
- Purpose: Standard types used across the system
- Contains: Msg, Query, Response types, Block info
- Key files: `lib.rs`, `api/mod.rs`

**gateway/:**
- Purpose: REST-to-gRPC translation (Go application)
- Contains: HTTP handlers, gRPC mux setup, CORS configuration
- Key files: `main.go` (entry), `cosmos/`, `layer/`, `cosmwasm/` (handlers)

**proto/:**
- Purpose: Protobuf source definitions
- Contains: `.proto` files for all message types
- Compiled into: `packages/proto/src/protos/`

**contracts/:**
- Purpose: Test/example smart contracts
- Contains: Root (privileged), Echo (test), Caller (test) contracts
- Not production code - for testing CosmWasm integration

**js/:**
- Purpose: TypeScript SDK and integration tests
- Contains: Test specs, utilities, faucet interactions
- Key files: `*.spec.ts` (integration tests), `testutils.spec.ts`

**localnode/:**
- Purpose: Local development environment setup
- Contains: Docker Compose orchestration, service configs
- Key files: `docker-compose.yml`, `abci/`, `comet/`

**docker/:**
- Purpose: Container images for deployment
- Contains: Dockerfiles for slay3rd, gateway, faucet
- Usage: Referenced in docker-compose and CI/CD

**scripts/:**
- Purpose: Build automation and deployment helpers
- Contains: Proto compilation, Docker build commands
- Key files: `build_proto_gateway`

## Key File Locations

**Entry Points:**
- `app/slay3rd/src/main.rs`: Binary entry point, config loading, server startup
- `gateway/main.go`: REST gateway entry point, gRPC mux setup
- `packages/app/src/app.rs`: State machine operations (queries, transactions)
- `packages/abci/src/server.rs`: ABCI socket server listening

**Configuration:**
- `app/slay3rd/src/config.rs`: Configuration parsing (TOML, env, CLI)
- `Cargo.toml`: Workspace members, dependencies, features
- `proto/`: Protobuf source files defining all message formats

**Core Logic:**
- `packages/app/src/sm.rs`: StateMachine dispatch logic
- `packages/app/src/auth/keeper.rs`: Signature validation, nonce tracking
- `packages/app/src/bank/keeper.rs`: Balance management
- `packages/app/src/wasm/keeper.rs`: Smart contract execution
- `packages/storage/src/traits.rs`: Storage interface definitions

**Testing:**
- `packages/app/src/testing/`: Test utilities and fixtures
- `js/src/*.spec.ts`: Integration tests via TypeScript

**Proto/gRPC:**
- `proto/`: Source `.proto` files
- `packages/proto/src/`: Compiled Rust definitions
- `gateway/`: gRPC service implementations (Go)
- `app/slay3rd/src/grpc/`: gRPC service implementations (Rust)

## Naming Conventions

**Files:**
- `keeper.rs`: Contains business logic for a module (Auth, Bank, Wasm)
- `mod.rs`: Re-exports public API for a module
- `error.rs`: Error type definitions
- `mod/mod.rs`: Submodule grouping
- `*.spec.ts`: TypeScript integration tests

**Directories:**
- `packages/`: Core libraries, published as separate crates
- `contracts/`: Smart contract implementations
- `app/`: Binary applications (runnable)
- `src/`: Source code root for Rust projects
- `grpc/`: gRPC service implementations

**Modules:**
- `auth`: Authentication and signature validation
- `bank`: Token balances and transfers
- `wasm`: CosmWasm integration
- `storage`: Persistence layer
- `proto`: Protocol definitions
- `std`: Standard types
- `abci`: ABCI protocol implementation

## Where to Add New Code

**New Feature (business logic):**
- Primary code: `packages/app/src/` - Add new module or extend existing (auth/bank/wasm)
- Tests: `packages/app/src/testing/` or new `tests/` directory
- Proto definitions: `proto/layer/` - Add new message types, regenerate
- gRPC handlers: `app/slay3rd/src/grpc/` - Implement query/tx handlers

**New Component/Module:**
- Implementation: `packages/{module_name}/src/lib.rs`
- Entry: Export public API from `mod.rs`
- Tests: Inline `#[cfg(test)]` or separate `tests/` directory

**New gRPC Service:**
- Proto definition: `proto/layer/` - Add service definition
- Rust impl: `app/slay3rd/src/grpc/{service}.rs` - Implement handler
- Go gateway: `gateway/{service}.go` - Auto-generated, regenerate from proto

**Utilities:**
- Shared helpers: `packages/std/src/` or `packages/golem/src/`
- Module-specific: `{module}/utils.rs` inside module

**Tests:**
- Unit: Inline `#[cfg(test)]` modules in source files
- Integration: `js/src/` for end-to-end, `packages/app/src/testing/` for app-level
- Storage tests: `packages/storage/src/tests/` (feature-gated per backend)

## Special Directories

**target/:**
- Purpose: Rust build artifacts
- Generated: Yes (created by `cargo build`)
- Committed: No (in .gitignore)

**node_modules/:**
- Purpose: JavaScript dependencies
- Generated: Yes (created by `npm install`)
- Committed: No (in .gitignore)

**.planning/codebase/:**
- Purpose: GSD codebase analysis documents
- Generated: Yes (by GSD mapper)
- Committed: Yes

**proto/generated/** (if present):**
- Purpose: Generated code from protobuf compilation
- Generated: Yes
- Committed: No (regenerated per build)

---

*Structure analysis: 2026-03-18*
