# Ewasm Design: Stateful Contracts on Layer

**Status:** Design exploration — pre-Phase 3
**Context:** The roadmap calls for Phase 3 (Ethereum type migration) and Phase 4 (CosmWasm VM replacement with wasmtime). This doc questions whether that path is right, and proposes alternatives.

---

## The Problem with the Current Plan

The roadmap has two expensive phases:

- **Phase 3** — Replace bech32 `Addr` with `alloy_primitives::Address` throughout every package, re-encode RocksDB keys, switch tx signing to ECDSA/keccak256 and ABI encoding
- **Phase 4** — Replace the CosmWasm VM (Wasmer + `cosmwasm_vm`) with a new wasmtime-based runtime; implement a custom cache, instance lifecycle, and host function interface from scratch; rewrite all contracts

The goal is Ethereum-native types end-to-end. But doing both phases is expensive, high-risk, and delays the project's primary value (Phase 5: WAVS integration) by a large margin.

The instinct to pause here is correct. Let's be precise about what's actually painful, what can be preserved, and what the alternatives look like.

---

## What We Have Today

### The good parts (worth keeping)

**The actor model works.** CosmWasm's instantiate/execute/query/migrate dispatch pattern is the right model for stateful WAVS contracts:
- Contracts have isolated storage namespaces
- Messages dispatch between contracts (cross-contract calls)
- Query is read-only and doesn't commit state
- Migrate handles schema upgrades

This is what WAVS contracts need: an AVS task queue, a verifier, and an operator registry are all simple state machines that accept messages and update state. The actor model handles this correctly.

**`AccountId` is already 20-byte.** The `AccountId` type in `packages/std/src/account_id.rs` stores raw bytes and accepts 20 or 32 byte addresses. It only uses bech32 at the `Display` and `parse_string` boundary. The storage layer (RocksDB, `layer_storage`) stores raw bytes — it doesn't care about encoding.

**`layer_std` has its own message types.** `Msg`, `WasmMsg`, `BankMsg`, `MsgData` in `packages/std/src/msg.rs` are already decoupled from `cosmwasm_std`. The CosmWasm types leak in at two specific points: (1) the VM backend traits (`BackendApi`, `BackendStorage`, `Querier`) and (2) the encoding in `keeper.rs` where `cosmwasm_std::Addr`, `BlockInfo`, `MessageInfo` are constructed for contract execution context.

**Wasmer works.** `cosmwasm_vm` bundles Wasmer and provides a battle-tested cache, instance lifecycle, and capability check system. The `VmCache` in `packages/app/src/wasm/vm/cache.rs` delegates to it directly. Replacing this with a custom wasmtime implementation means reimplementing ~2000 lines of VM infrastructure.

### The painful parts

**bech32 in the wire format.** The gRPC interface sends and receives bech32 addresses. `tx-sender` constructs them. The `AccountId::Display` impl formats as bech32. Changing this is a focused migration — maybe 15-20 call sites — but touches the public API.

**`cosmwasm_std` types in the execution context.** `keeper.rs:993-1001` constructs `cosmwasm_std::Env` and `MessageInfo` with `Addr::unchecked(contract.to_string())`. These are passed into the VM at execute/instantiate time. The contracts see bech32 addresses via `env.contract.address` and `info.sender`.

**JSON encoding in contracts.** CosmWasm contracts encode messages as JSON blobs (`Binary`). The host/contract boundary is JSON. ABI encoding is more efficient and Ethereum-tooling-compatible, but requires rewriting all contracts and the VM entry point dispatch.

**The unsafe transmute** in `vm/backend.rs:make_backend()` is still present. It was deemed safe by contract but is a footgun.

---

## Three Options

### Option A: Full Migration (Current Plan — Phases 3 + 4)

Replace bech32 with hex everywhere (Phase 3), then replace cosmwasm_vm/Wasmer with a custom wasmtime runtime (Phase 4).

**What it delivers:** Fully Ethereum-native Layer — hex addresses, ABI encoding, keccak256 signatures, wasmtime execution. The cleanest long-term architecture.

**What it costs:**
- Phase 3 alone touches `AccountId`, `keeper.rs`, the gRPC wire format, RocksDB key migration, tx signing, and all 4 existing contracts
- Phase 4 requires implementing: a WASM cache (module compilation, instance pooling), host function dispatch, gas metering, iterator support, capability checking — essentially a mini cosmwasm_vm from scratch
- All existing contracts (root, echo, counter) must be rewritten for the new runtime
- WAVS integration (Phase 5) is blocked until Phase 4 completes

**Risk:** High. The VM replacement is the riskiest part — cosmwasm_vm is ~5 years of battle-hardened edge case handling.

---

### Option B: Surgical Migration — Keep Wasmer, Change Encoding

Keep the actor model and Wasmer execution engine. Replace only the encoding layer: addresses become hex, tx signing uses ECDSA/keccak256, the host function interface strips Cosmos types but keeps the same Wasm entry point pattern.

**Specifically:**
1. Change `AccountId::Display` and `parse_string` from bech32 to `0x`-prefixed hex (20 bytes)
2. Remove `Addr` from the execution context — pass raw `AccountId` bytes to contracts via host functions
3. Replace the JSON entry point encoding with a thin ABI or msgpack envelope — or keep JSON but with hex addresses
4. Keep `cosmwasm_vm` as the VM (Wasmer + cache + instance lifecycle) — do NOT replace it
5. Keep the actor model (instantiate/execute/query/migrate) unchanged
6. Write new contracts against a minimal contract SDK that uses `AccountId` (hex, not bech32) and the existing entry point pattern

**What changes:** Encoding at the boundary. The host/contract interface still uses the same `call_execute`, `call_query` entry points — contracts just see hex addresses in `env.contract.address` instead of bech32.

**What stays the same:** Wasmer, cosmwasm_vm cache, instance lifecycle, gas metering, iterator support, capability checking, the actor model, storage namespacing.

**What this means for contracts:** Existing contracts need minor changes (address format). New contracts can use a stripped-down SDK that doesn't import `cosmwasm_std` at all — just a thin `layer-contract-sdk` crate with the host function stubs.

**Risk:** Low. The VM infrastructure is unchanged. The migration is bounded.

---

### Option C: WAVS-First Stateful Actors

Don't build a smart contract VM on Layer at all. Instead, extend WAVS components with persistent state — WAVS components declare storage namespaces, Layer manages their state lifecycle, and the "contracts" are just WAVS components with state.

**What this means concretely:**
- WAVS components are currently stateless (input → computation → output)
- Add a storage host function to the WAVS WASM ABI: `storage_read(key) -> value`, `storage_write(key, value)`
- Layer assigns each deployed component a storage namespace (keyed by component hash + deployer)
- "Deploy a contract" = upload a WASM component + reserve its storage namespace
- "Call a contract" = invoke a WAVS component with a storage context (reading/writing its namespace)

**Why this is appealing:** It's a simpler mental model. WAVS components are already Rust WASM programs compiled with standard tooling. You'd be extending the WAVS host ABI rather than building a separate smart contract runtime.

**The problems:**
- WAVS components today have no notion of caller authentication, message routing, or cross-component calls. The actor model provides all of this.
- You'd end up rebuilding CosmWasm's actor model (caller/callee separation, reentrancy guards, cross-contract dispatch, reply handling) inside the WAVS runtime.
- WAVS components are triggered by external events, not by on-chain transactions. The AVS operator registry needs to accept arbitrary calls from any address — this requires a transaction model, not a trigger model.
- The WAVS runtime is not something this project controls — it's an external system. Adding stateful storage to WAVS components would require upstream WAVS changes.

**Verdict:** The actor model features that CosmWasm provides are the right model for WAVS contracts on Layer. Option C ends up reinventing them under a different name, without the benefit of existing infrastructure.

---

## Recommendation: Option B, Phased

The instinct to avoid a painful migration is right, but the reason to avoid it is *scope*, not *direction*. The destination (Ethereum types, stateful actors on Layer) is correct. The current plan's Phase 4 (full VM replacement) is the expensive mistake.

### Revised Phase 3: Surgical Address Migration

Instead of "replace all types everywhere," do the minimum viable encoding change:

1. **`AccountId` encoding only:** Change `Display` to `0x`-hex, change `parse_string` to accept hex. Keep the `Vec<u8>` storage unchanged — this is a display/parse change, not a storage change.
2. **RocksDB keys:** Keys that encode `AccountId` (contract addresses, bank balances) are already raw bytes. No migration needed if `AccountId` internal representation doesn't change.
3. **gRPC wire format:** Update address fields from bech32 strings to hex strings. Update `tx-sender` to generate hex addresses.
4. **Execution context:** In `keeper.rs`, construct `Env`/`MessageInfo` with hex addresses instead of bech32 (`Addr::unchecked(hex_addr)` instead of `Addr::unchecked(bech32_addr)`). Contracts will see hex addresses.
5. **Tx signing:** Switch from Cosmos amino/protobuf signing to ECDSA/keccak256 (Ethereum wallet compatible). The wire format changes; the state machine logic doesn't.

**What this is NOT:** A full `alloy_primitives::Address` migration throughout every package. Use `AccountId` (already 20-byte bytes) as the canonical type. Only import `alloy_primitives` at the ECDSA signature verification boundary.

Estimated scope: ~8-12 files, no new dependencies beyond what's needed for ECDSA.

### Skip Phase 4 (VM Replacement)

Do not replace cosmwasm_vm/Wasmer. Instead:

- Write new WAVS contracts (task queue, verifier, operator registry) against a minimal `layer-contract-sdk` crate that provides idiomatic Rust contract entry points without cosmwasm_std boilerplate
- The SDK wraps the existing host function interface — contracts declare entry points as Rust functions, the SDK handles serialization
- Initially use JSON (CosmWasm-compatible); optionally switch to ABI encoding later when Ethereum wallet interaction is needed
- The existing VM infrastructure handles everything else

This unblocks Phase 5 (WAVS integration) immediately after Phase 3 without a multi-month VM rewrite.

### A Real Phase 4 (If Needed Later)

If wasmtime is needed (e.g., for SP1 zkVM compatibility in Phase 6, or for a constraint that cosmwasm_vm doesn't meet), scope it properly:

- Build the wasmtime host in isolation, behind a `VmBackend` trait
- Migrate the cache and instance lifecycle
- Port the existing contracts incrementally
- Keep the old Wasmer backend available behind a feature flag during migration

But this is Phase 6 territory, not a prerequisite for WAVS integration.

---

## The Actor Model for WAVS Contracts

Regardless of which option is chosen, the stateful contract model on Layer should preserve these properties from CosmWasm:

**Isolated storage namespaces.** Each contract instance gets a storage prefix keyed by its address. No contract can read or write another contract's storage except through explicit query calls.

**Authenticated caller.** Every execute/instantiate call includes `info.sender` — the address that signed the transaction. Contracts can enforce permissions based on caller address.

**Cross-contract calls.** Contracts can emit messages to other contracts via `CosmosMsg::Wasm(WasmMsg::Execute {...})`. The keeper dispatches these after the top-level message completes, handling reply callbacks.

**Deterministic address derivation.** `Instantiate2` with a salt produces the same address given the same deployer, code hash, and salt — equivalent to CREATE2. This enables pre-computed contract addresses.

**Query isolation.** Queries are read-only: they cannot modify state, and they execute against a snapshot of the committed state (not the in-progress block).

These properties are what WAVS operators need to interact with Layer safely. Preserving them is more important than the encoding format.

---

## Open Questions

1. **Do WAVS operators need Ethereum wallets to interact with Layer directly?** If yes, the tx signing change (ECDSA/keccak256) is urgent. If WAVS handles the signing and Layer just verifies operator BLS certificates, it's less urgent.

2. **Does the zkVM (SP1) need to re-execute Layer transactions?** If yes, the host function interface matters for the proof circuit — complex CosmWasm host functions are hard to re-implement in SP1 guest code. A thin host interface (just storage_read/write, send_message) would be easier to prove.

3. **What contracts does Phase 5 actually need?** If the WAVS integration contracts are simple state machines (accept a signed envelope, update a mapping, emit an event), they can be written in CosmWasm today without any migration. The question is whether the encoding (bech32 vs hex addresses) blocks WAVS operator tooling.

4. **Is the unsafe transmute in `vm/backend.rs` actually a problem?** The safety contract in the docstring appears sound — the `Backend` is consumed within the same stack frame. If `cargo miri test` passes on the VM tests, it's safe in practice even if it's aesthetically uncomfortable.

---

*Written: 2026-03-20 — pre-Phase 3 design review*
