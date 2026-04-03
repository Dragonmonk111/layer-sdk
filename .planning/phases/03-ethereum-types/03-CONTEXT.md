# Phase 3: Ethereum Types - Context

**Gathered:** 2026-04-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace Cosmos bech32 addresses with Ethereum 20-byte addresses (`alloy_primitives::Address`) throughout every package. Switch transaction signing to Ethereum ECDSA (secp256k1 + keccak256). Accept Ethereum RLP-encoded transactions natively. Re-encode RocksDB storage keys where needed without data loss. Strip Cosmos protobuf/amino encoding from the user-facing transaction path.

The driving constraint: **existing Ethereum wallets (MetaMask, Rabby, etc.) must be able to sign and submit transactions to the node natively.** This is not just a type cleanup — it's wallet compatibility.

</domain>

<decisions>
## Implementation Decisions

### Migration scope
- **D-01:** Full migration — `alloy_primitives::Address` replaces all address types throughout all packages. Not surgical/boundary-only. EWASM_DESIGN.md Option B (surgical) was considered and rejected in favor of the clean architecture.
- **D-02:** Phases 3+4 proceed as planned in the roadmap. Phase 4 (VM replacement) is NOT skipped.

### Address type strategy
- **D-03:** Replace `AccountId` (in `packages/std/src/account_id.rs`) with `alloy_primitives::Address` (20-byte, EIP-55 checksummed display). Clean break — `AccountId` is already 20-byte internally but carries bech32 encoding at Display/parse boundaries.
- **D-04:** `alloy_primitives::Address` becomes the canonical address type in auth, bank, wasm keeper, proto, and gRPC layers. No Cosmos `Addr` or bech32 encoding anywhere.

### Transaction format
- **D-05:** Node accepts Ethereum RLP-encoded transactions natively — the standard format that MetaMask and Ethereum wallets produce.
- **D-06:** EIP-155 chain ID included in transaction signatures for replay protection.
- **D-07:** Strip Cosmos protobuf/amino encoding from the user-facing transaction path entirely. Internal message dispatch format is Claude's discretion.
- **D-08:** Transaction payloads use ABI encoding through the full processing path (TYPES-04).

### Signing and extensibility
- **D-09:** Primary signing scheme: Ethereum ECDSA (secp256k1 + keccak256). This is the must-have for wallet compatibility.
- **D-10:** Signing architecture should be extensible to support other signature schemes in the future (e.g., BLS, ed25519, Schnorr). Design the verification interface to accept multiple signature types, with Ethereum ECDSA as the first and required implementation.

### Storage key migration
- **D-11:** Audit all RocksDB key prefixes that encode addresses. Where keys use bech32 string encoding, migrate to raw 20-byte `Address` encoding.
- **D-12:** Migration must be atomic via `WriteBatch` — no partial migration states. Silent data corruption is the highest-risk failure (flagged in STATE.md).
- **D-13:** Where `AccountId` internal representation is already raw bytes matching the new format, no migration is needed for those keys. Only keys that encode bech32 strings need re-encoding.

### Claude's Discretion
- Internal message dispatch format (between keeper and VM) — whatever is efficient
- `alloy` crate selection strategy (alloy-primitives, alloy-signer, alloy-rlp, alloy-consensus — pick what's needed)
- RocksDB migration implementation approach (inline migration on startup vs separate migration tool)
- Exact `cosmrs` removal strategy and replacement of its transitive tendermint dependency
- How to handle `packages/golem` (excluded from workspace since Phase 1 due to cw-orch-core v1→v2 breakage)
- Test contract updates (root, echo, caller) — update address format to hex

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase requirements and success criteria
- `.planning/ROADMAP.md` — Phase 3 goal, success criteria (TYPES-01 through TYPES-04), dependency on Phase 2.1
- `.planning/REQUIREMENTS.md` — TYPES-01 (address replacement), TYPES-02 (RocksDB migration), TYPES-03 (Ethereum ECDSA signing), TYPES-04 (ABI encoding)

### Design exploration (background context)
- `EWASM_DESIGN.md` — Analysis of migration options (A/B/C). Decision: Option A (full migration). Documents what `AccountId` stores internally, where bech32 leaks in, and the CosmWasm VM boundary points

### Address types (migration targets)
- `packages/std/src/account_id.rs` — Current `AccountId` type (raw bytes, bech32 Display/parse). Replace with `alloy_primitives::Address`
- `packages/app/src/wasm/keeper.rs` — Constructs `cosmwasm_std::Addr` and `Env`/`MessageInfo` for contract execution context. Major migration target.
- `packages/app/src/auth/keeper.rs` — Authentication and signature validation. Must switch to ECDSA/keccak256.
- `packages/app/src/bank/keeper.rs` — Balance tracking keyed by address. Storage keys may need migration.

### Transaction path (encoding migration targets)
- `packages/std/src/tx.rs` — `Tx` type definition. Must switch to RLP/ABI format.
- `packages/cosmos/src/tx.rs` — Cosmos tx deserialization (remove or replace with Ethereum RLP decoding)
- `packages/cosmos/src/query.rs` — Cosmos SDK gRPC query handlers (address format in query parameters)
- `packages/proto/src/protos/cosmos.tx.v1beta1.rs` — Cosmos tx proto types (remove/replace)

### Storage layer
- `packages/storage/src/` — Storage traits and implementations. Key encoding patterns.
- `packages/storage/src/rocks/mod.rs` — RocksDB implementation. WriteBatch for atomic migration.

### Node integration
- `app/slay3rd/src/main.rs` — Node entry point. Tx pipeline wiring.
- `app/slay3rd/src/node.rs` — `execute_block()` with tx deserialization
- `app/slay3rd/src/grpc/` — gRPC service handlers (address format in request/response)

### Prior phase state
- `.planning/STATE.md` — Accumulated decisions from Phases 1, 2, 2.1, 2.2. Key: "cosmrs 0.13.0 pulls tendermint 0.31.1 via layer-cosmos/layer-golem — Phase 3 replaces cosmrs entirely"

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `AccountId` in `packages/std/src/account_id.rs` — already stores raw 20-byte bytes internally. The struct can be replaced with a type alias to `alloy_primitives::Address` or removed entirely.
- `build_instantiate_2_address` in `packages/app/src/wasm/utils.rs` — CREATE2-style deterministic address derivation already exists. Update to use keccak256 instead of sha256 for Ethereum compatibility.
- Existing gRPC handlers in `app/slay3rd/src/grpc/` — structure stays, address format in request/response fields changes.

### Established Patterns
- `Arc<RwLock<App<T>>>` shared state model — unchanged by type migration
- `PersistentStorage` trait generic — RocksDB/MemoryStore agnostic
- `NodeConfig` via TOML/env/CLI (Figment) — add chain_id config
- `thiserror` for error types, `tracing` for logging — keep as-is

### Integration Points
- `keeper.rs` constructs `cosmwasm_std::Env`/`MessageInfo` — this is where Cosmos types leak into the VM. Phase 3 changes the address format passed in; Phase 4 replaces the entire VM boundary.
- `tx-sender` tool uses `layer-proto` (prost 0.13) — must be updated to construct RLP-encoded Ethereum transactions instead of Cosmos proto transactions.
- `packages/cosmos/` — most of this package may become obsolete or dramatically simplified. Cosmos query dispatch stays (renamed?), but Cosmos tx handling is replaced.

</code_context>

<specifics>
## Specific Ideas

- "Even though it's a WASM environment, we want it to support existing Ethereum wallets and standards." — This is the north star for Phase 3.
- Ethereum wallet compatibility is the driving requirement, not just a type cleanup. MetaMask, Rabby, ethers.js, viem should be able to interact with the node.
- Signing extensibility desired — Ethereum ECDSA first, but architecture should allow other schemes (BLS, ed25519) in the future.
- JSON-RPC endpoint (eth_sendRawTransaction etc.) is a v2 requirement (COMPAT-01) — not in Phase 3 scope, but the tx format laid here should make it straightforward to add later.

</specifics>

<deferred>
## Deferred Ideas

- JSON-RPC compatibility layer (eth_sendRawTransaction, eth_call, eth_getBalance) — v2 requirement COMPAT-01, not Phase 3 scope. Phase 3 lays the transaction format foundation.
- Solidity-to-WASM contract support (Revive/resolc) — v2 requirement COMPAT-02, separate from type migration.
- `packages/golem` cw-orch-core v1→v2 migration — excluded from workspace since Phase 1. Evaluate whether it's still needed after Ethereum type migration or can be dropped entirely.

</deferred>

---

*Phase: 03-ethereum-types*
*Context gathered: 2026-04-03*
