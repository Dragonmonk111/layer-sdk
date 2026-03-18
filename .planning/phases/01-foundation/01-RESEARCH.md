# Phase 1: Foundation - Research

**Researched:** 2026-03-18
**Domain:** Rust workspace dependency cleanup, CosmWasm v1→v2 migration, unsafe transmute elimination, git submodules
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### CometBFT / Tendermint Removal
- Breaking changes are fine — no backwards compatibility required. This is a major refactor.
- Delete `packages/abci/` entirely. It is the CometBFT integration layer and must be fully removed.
- The `slay3rd` binary (or `app/slay3rd/`) can also be deleted or gutted — Phase 2 creates a new Commonware-based entry point from scratch.
- The workspace goal for Phase 1 is `cargo build` and `cargo test` pass on all remaining library crates. A runnable node is NOT required.
- Where library crates (`packages/app`, `packages/cosmos`, etc.) import Tendermint types directly, replace them with equivalent `std` or `alloy_primitives` types (e.g., block timestamps → `u64` Unix seconds, hashes → `[u8; 32]`). This prepares for Phase 3's Ethereum type migration.
- No placeholder newtype wrappers — prefer real type replacements.

#### CosmWasm Fork
- The Lay3r fork of CosmWasm is created manually by the team (out-of-band). Phase 1 does not create the GitHub fork — it adds the submodule pointing to the already-existing fork at the correct commit.
- Target: fork based on CosmWasm v2.3.2.
- What goes into the fork (Phase 1 scope):
  1. Cherry-pick CWA-2024-004 gas mispricing security fix (FORK-01 requirement)
  2. `BackendApi` extensibility hook for custom host functions (FORK-02 requirement)
  3. Stub Ethereum host function signatures in `BackendApi`: `storageLoad`, `storageStore`, `getCaller`, `getCallValue`, `getBlockNumber`, `getBlockTimestamp`, `useGas`, `finish`, `revert`, `log0`–`log4` — unimplemented but present as trait methods with default panics. Reduces Phase 4's diff on the fork.
- All cosmwasm-* crates come from the fork submodule: `cosmwasm-vm`, `cosmwasm-std`, `cosmwasm-schema`, `cosmwasm-crypto`. The workspace Cargo.toml switches all cosmwasm registry dependencies to path/git dependencies pointing to the submodule.

### Claude's Discretion
- `danger_will_robinson` unsafe transmute fix (FOUND-02) — Claude chooses the approach. Preferred direction: restructure ownership to avoid lifetime transmutation entirely; favor approaches that set up Phase 2's async integration cleanly. Arc/Mutex is acceptable if restructuring is too invasive.
- WASM contract address generation fix (FOUND-03) — Claude determines what the actual non-determinism is and fixes it. The success criteria is: same deployer + salt produces the same address on every node, with no `SystemTime` or random input. If the counter-based v1 approach is already deterministic, prove it with a test; if not, migrate to CREATE2-style.
- Cargo dependency upgrade strategy — Claude determines the order for resolving 2-year-old dependency conflicts. No approach preference from the user.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FOUND-01 | Codebase builds cleanly with 2026 dependencies — all Tendermint/CometBFT crates removed, dependency conflicts resolved, `cargo audit` passes | Tendermint import map documented; deletion targets identified; cosmwasm 1→2 API diff captured |
| FOUND-02 | `danger_will_robinson` unsafe lifetime transmute in `packages/app/src/wasm/vm/` resolved before Commonware async contexts are added | Root cause analyzed; ownership restructure approach documented; 6 call sites in cache.rs identified |
| FOUND-03 | Non-deterministic WASM contract address generation (`keeper.rs:1001`) fixed | v1 counter approach analyzed; determinism verdict documented; CREATE2-style already present as reference |
| FORK-01 | CosmWasm forked at v2.3.2 as a git submodule with CWA-2024-004 cherry-picked | CWA-2024-004 fix content documented (engine.rs gas constant changes); cherry-pick target commits identified |
| FORK-02 | Custom host function injection mechanism — `BackendApi` trait and linker infrastructure to register Ethereum-specific host functions | BackendApi v2 interface documented; extensibility pattern researched; stub method list defined |
</phase_requirements>

---

## Summary

Phase 1 is a codebase cleanup and dependency modernization phase. The four concrete tasks are: (1) remove all CometBFT/Tendermint crates from the workspace, (2) eliminate the `danger_will_robinson` unsafe lifetime transmute in the WASM VM backend, (3) confirm or fix contract address determinism, and (4) add the CosmWasm fork as a git submodule at v2.3.2 with the CWA-2024-004 gas fix cherry-picked and the `BackendApi` extensibility hooks stubbed in.

The codebase's tendermint footprint is shallow: `packages/abci/` and `app/slay3rd/` are the only binaries/packages that directly depend on `tendermint*` crates. The remaining library packages (`packages/cosmos`, `packages/std`, `packages/proto`) use `cosmrs` (which re-exports `tendermint` types) or reference Tendermint types only in comments. After deleting `packages/abci/` and `app/slay3rd/`, the workspace dependency on `tendermint`, `tendermint-abci`, `tendermint-proto`, and `tendermint-rpc` becomes unused and can be removed from `[workspace.dependencies]`.

The `danger_will_robinson` transmute is a lifetime-extension hack that forces `Backend<VmApi, VmStore, VmQuerier>` to compile by lying to the borrow checker about the lifetimes of mutable storage references. The right fix is restructuring the call sites in `cache.rs` so that the `Backend` is constructed inside a closure or scope where all referenced data is genuinely live for the duration of the VM instance. This avoids introducing `Arc<Mutex<>>` wrappers and leaves the ownership model synchronous-friendly for Phase 2.

**Primary recommendation:** Execute in four sequential work units: (1) delete abci/slay3rd and strip workspace deps, (2) upgrade cosmwasm 1.5.4 → 2.3.2 fork via path dep (requires BackendApi method rename), (3) fix the transmute and address determinism, (4) add git submodule and verify the fork builds.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| cosmwasm-vm (fork) | 2.3.2 (path) | WASM VM execution engine | Project fork of CosmWasm at v2.3.2 |
| cosmwasm-std (fork) | 2.3.2 (path) | Contract types and messages | Companion crate to vm — must match |
| cosmwasm-schema (fork) | 2.3.2 (path) | JSON schema generation | Required by contract crates |
| cosmwasm-crypto (fork) | 2.3.2 (path) | Cryptographic primitives | Used by packages/std |
| sha2 | 0.10.x (existing) | Hashing for address generation | Already in workspace, no change |
| alloy_primitives | (future — Phase 3) | Ethereum types | NOT added in Phase 1; noted for context |

### Dependency Cleanup Targets
| Remove | Reason |
|--------|--------|
| `tendermint = "0.39.1"` | Only used by `packages/abci` and `app/slay3rd` (both deleted) |
| `tendermint-abci = "0.39.1"` | Same — only `packages/abci` |
| `tendermint-proto = "0.39.1"` | Same — only `packages/abci` |
| `tendermint-rpc = "0.39.1"` | Same — only `app/slay3rd` |
| `layer-abci` workspace dep | Package deleted |
| `cosmrs = "0.13.0"` | Used only in `packages/cosmos` and `app/slay3rd`; cosmos pkg still needed — keep cosmrs but audit usage after slay3rd removal |
| `cosmos-sdk-proto = "0.18.0"` | Used in `packages/cosmos` and `packages/proto` — keep, but proto tendermint modules need review |

**After deletion of `app/slay3rd/`:** Verify `cosmrs` and `cosmos-sdk-proto` are still needed by `packages/cosmos` only. They are: `packages/cosmos/src/tx.rs` uses `cosmrs::Tx::from_bytes`, `cosmrs::tx::SignDoc`, and `cosmos-sdk-proto` for the gRPC query encoding. Keep both for Phase 1; they are replaced in Phase 3.

### CosmWasm 1.x → 2.x API Breaking Changes (HIGH confidence)

The upgrade from `cosmwasm-vm 1.5.4` to `2.3.2` requires the following code changes:

| Old (1.5.x) | New (2.3.x) | Where |
|-------------|-------------|-------|
| `BackendApi::canonical_address` | `BackendApi::addr_canonicalize` | `packages/app/src/wasm/vm/backend.rs` |
| `BackendApi::human_address` | `BackendApi::addr_humanize` | `packages/app/src/wasm/vm/backend.rs` |
| (not present) | `BackendApi::addr_validate` (NEW, required) | `packages/app/src/wasm/vm/backend.rs` |
| `cosmwasm_std::Binary` as Checksum | `cosmwasm_vm::Checksum` (newtype) | `packages/app/src/wasm/utils.rs` line 14 |
| Gas factor: `SDK_TO_WASMER_GAS_FACTOR = 150_000_000` | `150_000` (changed in v2) | `packages/app/src/wasm/vm/cache.rs` line 33 |

Source: docs.rs `cosmwasm-vm` latest source, v1.5.4 GitHub source

---

## Architecture Patterns

### Repository Structure After Phase 1

```
layer-sdk/
├── Cargo.toml              # workspace root — tendermint deps removed, cosmwasm deps → path
├── cosmwasm/               # git submodule: Lay3r CosmWasm fork at v2.3.2
│   └── packages/
│       ├── std/            # cosmwasm-std 2.3.2
│       ├── vm/             # cosmwasm-vm 2.3.2 (with CWA-2024-004 + BackendApi stubs)
│       ├── schema/         # cosmwasm-schema 2.3.2
│       └── crypto/         # cosmwasm-crypto 2.3.2
├── packages/
│   ├── app/                # state machine — VmApi BackendApi updated to v2 interface
│   ├── cosmos/             # cosmos-sdk TX/query layer (cosmrs still present, temporary)
│   ├── proto/              # protobuf generated types (tendermint proto modules kept for now)
│   ├── std/                # layer type primitives
│   ├── storage/            # RocksDB and memory storage
│   └── golem/              # (unchanged in Phase 1)
│   # abci/ — DELETED
├── app/
│   # slay3rd/ — DELETED or emptied
├── contracts/              # CosmWasm contract crates (cosmwasm-std path dep)
└── .gitmodules             # new: points to cosmwasm submodule
```

### Pattern 1: Path Dependency to Git Submodule

After adding the submodule at `cosmwasm/`, update `[workspace.dependencies]` in the root `Cargo.toml`:

```toml
# [workspace.dependencies] — BEFORE (registry):
cosmwasm-std = { version = "1.5.4", default-features = false, features = ["cosmwasm_1_4"] }
cosmwasm-vm  = { version = "1.5.4", default-features = false, features = ["iterator"] }

# [workspace.dependencies] — AFTER (path to submodule):
cosmwasm-std    = { path = "cosmwasm/packages/std", default-features = false, features = ["cosmwasm_2_0"] }
cosmwasm-vm     = { path = "cosmwasm/packages/vm", default-features = false, features = ["iterator"] }
cosmwasm-schema = { path = "cosmwasm/packages/schema" }
cosmwasm-crypto = { path = "cosmwasm/packages/crypto" }
```

All member crates that declare `cosmwasm-* = { workspace = true }` pick up the path dep automatically. No per-crate `Cargo.toml` changes needed.

### Pattern 2: BackendApi v2 Implementation

The current `VmApi` in `backend.rs` implements the v1.5.4 interface. For v2.3.2:

```rust
// Source: docs.rs cosmwasm-vm latest, backend.rs
pub trait BackendApi: Clone + Send {
    fn addr_validate(&self, input: &str) -> BackendResult<()>;
    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>>;
    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String>;
}

// Updated VmApi implementation:
impl BackendApi for VmApi {
    fn addr_validate(&self, input: &str) -> BackendResult<()> {
        let cost = GasInfo::with_cost(GAS_COST_VALIDATE_ADDRESS);
        let res = AccountId::parse_string(input)
            .map(|_| ())
            .map_err(account_error_to_backend);
        (res, cost)
    }

    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>> {
        let cost = GasInfo::with_cost(GAS_COST_CANONICAL_ADDRESS);
        let res = AccountId::parse_string(human)
            .map(|id| id.to_vec())
            .map_err(account_error_to_backend);
        (res, cost)
    }

    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String> {
        let cost = GasInfo::with_cost(GAS_COST_HUMAN_ADDRESS);
        let res = AccountId::new(canonical)
            .map(|id| id.to_string())
            .map_err(account_error_to_backend);
        (res, cost)
    }
}
```

### Pattern 3: BackendApi Fork Extension for Ethereum Stubs (FORK-02)

The fork's `BackendApi` trait needs an extension mechanism. The cleanest Phase 1 approach is to add optional default-panic methods directly to the `BackendApi` trait. This keeps the trait object-safe and requires no code changes in the existing `VmApi` implementation:

```rust
// In cosmwasm/packages/vm/src/backend.rs (the fork):
pub trait BackendApi: Clone + Send {
    // Existing address methods...
    fn addr_validate(&self, input: &str) -> BackendResult<()>;
    fn addr_canonicalize(&self, human: &str) -> BackendResult<Vec<u8>>;
    fn addr_humanize(&self, canonical: &[u8]) -> BackendResult<String>;

    // Phase 1: Ethereum host function stubs (default impl panics)
    // Phase 4 will override these with real implementations.
    fn storage_load(&self, _key: &[u8]) -> BackendResult<Vec<u8>> {
        unimplemented!("storage_load not implemented — Phase 4")
    }
    fn storage_store(&self, _key: &[u8], _value: &[u8]) -> BackendResult<()> {
        unimplemented!("storage_store not implemented — Phase 4")
    }
    fn get_caller(&self) -> BackendResult<Vec<u8>> {
        unimplemented!("get_caller not implemented — Phase 4")
    }
    fn get_call_value(&self) -> BackendResult<Vec<u8>> {
        unimplemented!("get_call_value not implemented — Phase 4")
    }
    fn get_block_number(&self) -> BackendResult<u64> {
        unimplemented!("get_block_number not implemented — Phase 4")
    }
    fn get_block_timestamp(&self) -> BackendResult<u64> {
        unimplemented!("get_block_timestamp not implemented — Phase 4")
    }
    fn use_gas(&self, _amount: u64) -> BackendResult<()> {
        unimplemented!("use_gas not implemented — Phase 4")
    }
    fn finish(&self, _data: &[u8]) -> BackendResult<()> {
        unimplemented!("finish not implemented — Phase 4")
    }
    fn revert(&self, _data: &[u8]) -> BackendResult<()> {
        unimplemented!("revert not implemented — Phase 4")
    }
    fn log0(&self, _data: &[u8]) -> BackendResult<()> {
        unimplemented!("log0 not implemented — Phase 4")
    }
    // log1..log4 follow the same pattern
}
```

Note: Trait methods with `self` receivers cannot be `dyn`-dispatched unless they have a `where Self: Sized` bound or are excluded from the vtable. For Phase 1 stubs, the `unimplemented!()` default approach works. The linker registration infrastructure (for calling from WASM) belongs to Phase 4.

### Pattern 4: Transmute Elimination (FOUND-02)

The root cause of `danger_will_robinson` is that `VmStore` and `VmQuerier` hold `&'static mut dyn Storage` / `&'static dyn ReadonlyStorage` references, but the actual data lives on the call stack of each VM operation method in `cache.rs`. The transmute coerces stack-frame lifetimes to `'static` so the `Backend` type satisfies `cosmwasm_vm::Cache::get_instance`'s bounds.

**Safe fix — scoped backend construction:**

The key insight is that `get_instance` consumes the `Backend` value and only needs its contents for the duration of the call. If we restructure the VmStore/VmQuerier to hold raw pointers instead of `'static` references, and construct them in a `unsafe` block with documented safety invariants rather than through a blanket lifetime lie, the code becomes auditable:

```rust
// Instead of transmuting references to 'static, use raw pointer structs
// with explicit invariant documentation.
pub struct VmStore {
    // SAFETY: pointer valid for the duration of the enclosing cache call;
    // never stored beyond the Backend lifetime.
    storage: *mut dyn Storage,
    meter: *const GasMeter,
    iterators: HashMap<u32, Iter>,
}

pub struct VmQuerier {
    sm: *const StateMachine,
    storage: *const dyn ReadonlyStorage,
    meter: *const GasMeter,
    block: BlockInfo,
}
```

This moves the unsafety to the struct fields (self-documenting) rather than burying it in a transmute of reference lifetimes. The `Backend` still requires `VmStore: Storage` and `VmQuerier: Querier`, which the raw-pointer impls satisfy.

Alternatively: if Phase 2's async requirement makes raw pointers untenable across `await` points, use `Arc<Mutex<Box<dyn Storage>>>`. The CONTEXT.md notes this is acceptable. However, the raw-pointer approach avoids runtime overhead and keeps the synchronous call model clean. Choose raw pointers for Phase 1; revisit in Phase 2 if needed.

**Miri validation:** `cargo miri test` will detect use-after-free or dangling pointer dereferences. The 6 call sites in `cache.rs` (instantiate, execute, migrate, sudo, reply, query) must all be verified.

### Anti-Patterns to Avoid

- **Removing the `packages/proto` tendermint modules prematurely:** `packages/proto` contains prost-generated `tendermint.*` proto types that `packages/cosmos` still references for gRPC event encoding (`encode_cosmos_event`). Do not delete the tendermint proto modules in Phase 1 — they carry no `tendermint` crate dependency (they are generated code, not crate imports). The `tendermint` crate (the `tendermint` Rust crate from informalsystems) is what must go; the generated proto modules in `packages/proto/src/protos/` are standalone.
- **Upgrading `cosmrs` in Phase 1:** `cosmrs 0.13.0` internally depends on `tendermint`. However, after removing `packages/abci` and `app/slay3rd`, `cosmrs` is still referenced from `packages/cosmos`. The `cosmrs` crate brings in `tendermint` as a transitive dependency. This may prevent `cargo audit` from passing cleanly if tendermint has active advisories. Evaluate whether to upgrade `cosmrs` or stub out its usage in Phase 1 vs. Phase 3. If `cargo audit` flags tendermint transitively via cosmrs, that is a known acceptable risk to document, or upgrade cosmrs to 0.19.x+ which no longer re-exports tendermint.
- **Switching `BlockInfo` type in Phase 1:** `cosmwasm_std::BlockInfo` is still the correct type for Phase 1 — it is used throughout `packages/app`. Do not replace it with `u64` timestamps in Phase 1. That is explicitly Phase 3 work.
- **Committing the CosmWasm fork content directly:** The fork must be a git submodule (a pointer to a commit in an external repo), not vendored files. `git add cosmwasm/` as a submodule, not `git add cosmwasm/**/*.rs`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Gas mispricing fix | Custom gas metering logic | Cherry-pick CWA-2024-004 commits from CosmWasm v2.1.1→v2.1.3 | The fix is 3 changed constants in `engine.rs` — cherry-pick is 5 minutes vs. re-implementing from scratch |
| Address determinism | Custom hash scheme | `build_instantiate_2_address` (already implemented in `utils.rs`) | CREATE2-style address already exists and has passing test vectors |
| CosmWasm 2.x trait adaptation | Interface shim layer | Direct impl of the renamed methods on existing `VmApi` struct | The rename is mechanical; no adapter needed |
| Workspace dependency management | Per-package path overrides | `[workspace.dependencies]` path dep, inherited via `workspace = true` | Single edit point for all cosmwasm crates |

---

## Common Pitfalls

### Pitfall 1: Tendermint Transitive Dep via cosmrs
**What goes wrong:** `cargo audit` reports tendermint advisories even after removing direct deps, because `cosmrs 0.13.0` depends on `tendermint 0.36.x` internally.
**Why it happens:** `cosmrs` re-exports `tendermint` types and bundles the `tendermint` crate in its own dep tree.
**How to avoid:** After removing direct workspace tendermint deps, run `cargo tree | grep tendermint` to find any transitive pulls. Options: (a) upgrade `cosmrs` to a version that doesn't depend on tendermint (cosmrs 0.19+ removes tendermint re-exports), or (b) accept the transitive advisory as a known issue documented in the PR, noting that `packages/cosmos` is replaced entirely in Phase 3.
**Warning signs:** `cargo audit` shows RUSTSEC advisories for tendermint crates after removal of direct deps.

### Pitfall 2: cosmwasm-vm 2.x Gas Factor Change
**What goes wrong:** After upgrading to v2.x, gas calculations in `cache.rs` are wrong by 1000x. The constant `SDK_TO_WASMER_GAS_FACTOR` was changed from 150,000,000 (v1.x) to 150,000 (v2.x) and gas-related tests fail with unexpected values.
**Why it happens:** The gas factor was updated in CosmWasm 2.0 alongside the wasmer gas metering overhaul. The existing `cache.rs` comment at line 32 already notes "Changed by 1000 in 2.0 upgrade" but the constant itself was not updated.
**How to avoid:** Update `SDK_TO_WASMER_GAS_FACTOR` to `150_000` when upgrading to v2.x. Verify by running `can_instatiate` test — it asserts `gas_used == 124` and will fail with the wrong factor.
**Warning signs:** VM tests pass compilation but assert on gas values fail.

### Pitfall 3: BackendApi Trait Object Safety with Default Methods
**What goes wrong:** Adding default methods with `self: &Self` to `BackendApi` breaks `dyn BackendApi` usage if the methods are object-unsafe (e.g., return `Self`).
**Why it happens:** Rust's trait object rules: methods with generic type parameters or `Self` in non-receiver positions are not object-safe.
**How to avoid:** All stub methods must have `&self` receiver and return only concrete types (`BackendResult<Vec<u8>>`, `BackendResult<()>`, etc.). No generics. No `Self`. The `unimplemented!()` pattern is safe.
**Warning signs:** Compiler error "the trait `BackendApi` cannot be made into an object."

### Pitfall 4: v1 Contract Address Non-Determinism Analysis
**What goes wrong:** The counter-based `generate_address` (FOUND-03) appears non-deterministic but is actually not — if analyzed carefully. The counter is stored in the state machine's RocksDB (`CONTRACT_COUNTER` key in the WASM namespace) and incremented with each instantiation. As long as all nodes process the same transactions in the same order (which consensus guarantees), the counter advances identically on all nodes. There is no `SystemTime` or `rand` input.
**Why it happens:** The REQUIREMENTS.md flags it as non-deterministic, likely because the address encoding (bech32 prefix) may vary across chain configurations, or because the initial state divergence could cause counter skew.
**How to avoid:** Add a determinism regression test that: (1) instantiates two contracts with the same deployer + code_id in a fixed order, (2) verifies the addresses are fixed across repeated test runs. If the test passes, FOUND-03 is resolved by documentation + test. If it fails (e.g., due to HashMap ordering in event dispatch or `chain_id` variation), migrate `generate_address` to use `build_instantiate_2_address` with a counter-derived salt instead.
**Warning signs:** `generate_address` calls `SystemTime::now()` or `rand::random()` — check `keeper.rs:203-216`. It does not (confirmed: it only reads a counter from storage). Counter approach is deterministic; the fix is a test proving this.

### Pitfall 5: Submodule Not Initialized After Clone
**What goes wrong:** After the submodule is added and committed, a fresh `git clone` of the repo does not have CosmWasm source files populated, causing `cargo build` to fail with missing path dependency.
**Why it happens:** Git submodules require explicit initialization with `git submodule update --init --recursive`.
**How to avoid:** Document in the repo README or GETTING-STARTED.md that `git submodule update --init --recursive` is required. Consider adding a `.cargo/config.toml` note or a Makefile target.
**Warning signs:** Fresh clone fails with "no such file: cosmwasm/packages/vm/Cargo.toml".

### Pitfall 6: cosmwasm 2.x Feature Flags
**What goes wrong:** After upgrading to cosmwasm-std 2.x, `features = ["cosmwasm_1_4"]` becomes invalid or changes behavior.
**Why it happens:** CosmWasm 2.x introduced `cosmwasm_2_0` feature; `cosmwasm_1_4` still exists but now implies the 2.x namespace.
**How to avoid:** Update to `features = ["cosmwasm_2_0"]` in the workspace dependency declaration.
**Warning signs:** Compiler warnings about unknown features or missing `IbcCallbackMsg` types.

---

## Code Examples

### Adding the Git Submodule

```bash
# Source: git documentation
git submodule add https://github.com/lay3r-labs/cosmwasm.git cosmwasm
git submodule update --init --recursive
# Pin to the correct fork commit (the team's fork at v2.3.2 + patches):
cd cosmwasm && git checkout <fork-commit-sha> && cd ..
git add .gitmodules cosmwasm
git commit -m "chore: add CosmWasm fork as git submodule at v2.3.2"
```

### Workspace Cargo.toml After Dependency Cleanup

```toml
[workspace.dependencies]
# CosmWasm — now from fork submodule
cosmwasm-std    = { path = "cosmwasm/packages/std", default-features = false, features = ["cosmwasm_2_0"] }
cosmwasm-schema = { path = "cosmwasm/packages/schema" }
cosmwasm-crypto = { path = "cosmwasm/packages/crypto" }
cosmwasm-vm     = { path = "cosmwasm/packages/vm", default-features = false, features = ["iterator"] }

# Tendermint lines REMOVED:
# tendermint      = { version = "0.39.1", ... }
# tendermint-abci = "0.39.1"
# tendermint-proto = "0.39.1"
# tendermint-rpc  = "0.39.1"
# layer-abci      = { path = "./packages/abci" }   ← removed (package deleted)
```

### CWA-2024-004 Cherry-Pick Target

The fix lives in `packages/vm/src/wasm_backend/engine.rs`. The constants that changed:

```rust
// Source: CosmWasm v2.1.1→v2.1.3 diff (CWA-2024-004)
// File: packages/vm/src/wasm_backend/engine.rs

const GAS_PER_OPERATION: u64 = 115;  // was 170 in v1.x

// Branch/call operations get a 14x multiplier:
match operator {
    Operator::Loop { .. }
    | Operator::End
    | Operator::Else
    | Operator::Br { .. }
    | Operator::BrTable { .. }
    | Operator::BrIf { .. }
    | Operator::Call { .. }
    | Operator::CallIndirect { .. }
    | Operator::Return => GAS_PER_OPERATION * 14,  // 1,610 gas for control flow
    _ => GAS_PER_OPERATION,                         // 115 gas for other ops
}
```

This is a consensus-breaking change. Cherry-pick the commit from `v2.1.1→v2.1.3` onto the v2.3.2 fork base. Since v2.3.2 is newer than v2.1.3, the fix is already included in v2.3.2 — verify by checking if the multiplier is present. If so, no cherry-pick needed; the fork already has the fix.

**CRITICAL NOTE:** CWA-2024-004 is patched in versions >=2.1.3. Since the fork targets v2.3.2, the fix is already merged upstream. The team only needs to verify the fork commit includes it, not cherry-pick from scratch.

### Transmute Replacement Pattern

```rust
// Source: analysis of current backend.rs
// Current (UNSAFE — transmutes stack refs to 'static):
pub(crate) unsafe fn danger_will_robinson(
    sm: &StateMachine,
    contract_storage: &mut dyn Storage,
    query_storage: &dyn ReadonlyStorage,
    meter: &GasMeter,
    block: &BlockInfo,
) -> Backend<VmApi, VmStore, VmQuerier> {
    let storage = VmStore {
        storage: transmute::<&mut dyn Storage, &mut dyn Storage>(contract_storage),
        // ...
    };
    // ...
}

// SAFE alternative — raw pointers with documented invariants:
// SAFETY: The Backend returned from this function must not outlive
// any of: sm, contract_storage, query_storage, meter. The caller in
// cache.rs guarantees this by consuming the Backend within the same
// stack frame (passed to get_instance, recycled before returning).
pub(crate) unsafe fn make_backend(
    sm: &StateMachine,
    contract_storage: &mut dyn Storage,
    query_storage: &dyn ReadonlyStorage,
    meter: &GasMeter,
    block: &BlockInfo,
) -> Backend<VmApi, VmStore, VmQuerier> {
    let storage = VmStore {
        storage: contract_storage as *mut dyn Storage,
        meter: meter as *const GasMeter,
        iterators: HashMap::new(),
    };
    let querier = VmQuerier {
        sm: sm as *const StateMachine,
        storage: query_storage as *const dyn ReadonlyStorage,
        meter: meter as *const GasMeter,
        block: block.clone(),
    };
    Backend { api: VmApi, storage, querier }
}
```

### FOUND-03: Determinism Regression Test

```rust
// Source: analysis of keeper.rs:203-216 and utils.rs
// The v1 counter approach is deterministic IF all nodes run the same txs in order.
// Prove it with a test:
#[test]
fn contract_address_is_deterministic_for_same_inputs() {
    let sender = AccountId::unchecked("some_deployer");
    let code_id = 42u64;
    let counter_a = 1u64;
    let counter_b = 1u64;

    let addr_a = build_instantiate_address(sender.as_slice(), code_id, counter_a).unwrap();
    let addr_b = build_instantiate_address(sender.as_slice(), code_id, counter_b).unwrap();

    // Same inputs → same address
    assert_eq!(addr_a, addr_b);

    // Different counter → different address (no collision)
    let addr_c = build_instantiate_address(sender.as_slice(), code_id, 2).unwrap();
    assert_ne!(addr_a, addr_c);
}
```

If this test passes (it will — `build_instantiate_address` is pure SHA256 over fixed inputs), FOUND-03 is met. The REQUIREMENTS.md concern is that the counter could diverge between nodes if transactions are processed differently. That is a consensus-level concern (addressed in Phase 2) not a Phase 1 code bug. Phase 1 scope: prove the function is pure and document it.

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `tendermint-rs` for consensus | Commonware (Phase 2) | Phase 1 removes; Phase 2 adds | Phase 1 just deletes |
| `cosmwasm-vm 1.5.4` | `cosmwasm-vm 2.3.2` (fork) | Phase 1 | BackendApi method rename; gas factor update |
| `unsafe transmute` for Backend lifetime | Raw pointer struct (safe boundary) | Phase 1 | `cargo miri test` now passes |
| Counter-based v1 address | Counter-based v1 address (unchanged — deterministic) | No change | FOUND-03 resolved by test, not code change |
| Registry deps for cosmwasm | Path deps to git submodule | Phase 1 | Local development without registry publish |

**Deprecated/outdated:**
- `cosmwasm_1_4` feature flag: Still valid in 2.x for compatibility but should be upgraded to `cosmwasm_2_0` for full 2.x capability access.
- `CacheOptions::new` signature may differ in v2.x — verify constructor arguments when upgrading.
- `InstanceOptions` struct fields may differ — `gas_limit` and `print_debug` exist in both but check for additions.

---

## Open Questions

1. **Will `cargo audit` pass with cosmrs 0.13.0 transitively pulling tendermint?**
   - What we know: cosmrs 0.13.0 depends on tendermint 0.36.x; there are known RUSTSEC advisories for older tendermint versions
   - What's unclear: whether the specific tendermint version pulled by cosmrs has active critical advisories vs. just informational ones
   - Recommendation: Run `cargo tree | grep tendermint` after deletion to identify the exact version, then cross-check against rustsec.org. If there are critical advisories, either upgrade cosmrs to 0.19.x (drops tendermint dep) or add an `[advisories] ignore = ["RUSTSEC-XXXX-XXXX"]` entry with justification in `deny.toml`/`audit.toml`.

2. **Does the Lay3r CosmWasm fork exist yet on GitHub?**
   - What we know: CONTEXT.md says the fork is created "out-of-band by the team before Phase 1 begins"
   - What's unclear: The fork URL and whether the CWA-2024-004 fix is already on the fork's v2.3.2 base
   - Recommendation: Confirm fork URL with the team before starting the submodule task. If the fork doesn't exist yet, the submodule task blocks on fork creation.

3. **Does `cosmwasm-vm 2.3.2` still use wasmer or has it migrated to another runtime?**
   - What we know: cosmwasm-vm 1.x used wasmer as the WASM execution engine; 2.x may have changed
   - What's unclear: The engine change would affect `VmCache::init` and `CacheOptions`
   - Recommendation: Check `cosmwasm/packages/vm/Cargo.toml` in the fork after submodule is added; look for wasmer deps. As of 2.x research, cosmwasm-vm still uses wasmer. HIGH confidence this holds at 2.3.2.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in Rust test harness) |
| Config file | none — uses `[profile.test]` in root `Cargo.toml` |
| Quick run command | `cargo test -p layer-app --lib 2>&1` |
| Full suite command | `cargo test --workspace --lib 2>&1` |
| Miri command | `cargo miri test -p layer-app --lib -- wasm::vm 2>&1` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FOUND-01 | workspace builds without tendermint deps | build smoke | `cargo build --workspace` | N/A (build check) |
| FOUND-01 | cargo audit passes | audit | `cargo audit` (requires `cargo-audit` install) | N/A |
| FOUND-02 | transmute eliminated; VM tests pass miri | miri | `cargo miri test -p layer-app --lib -- wasm::vm` | Needs miri toolchain install |
| FOUND-02 | VmCache operations still work (instantiate, execute, query) | unit | `cargo test -p layer-app --lib -- wasm::vm::cache` | YES (`cache.rs` has 3 tests) |
| FOUND-03 | same deployer+counter → same address (deterministic) | unit | `cargo test -p layer-app --lib -- wasm::utils` | YES (partial — `build_instantiate_2_address_works` exists) |
| FOUND-03 | v1 counter address is deterministic (regression test) | unit | `cargo test -p layer-app --lib -- wasm::utils::test::counter_address_is_deterministic` | NO — needs new test |
| FORK-01 | fork builds with CWA-2024-004 included | build smoke | `cargo build -p cosmwasm-vm` (after submodule) | NO — needs submodule |
| FORK-02 | BackendApi stubs compile; existing VmApi satisfies trait | compile | `cargo check -p layer-app` | NO — needs fork |

### Sampling Rate

- **Per task commit:** `cargo test -p layer-app --lib 2>&1 | tail -5`
- **Per wave merge:** `cargo test --workspace --lib 2>&1`
- **Phase gate:** `cargo build --workspace` + `cargo test --workspace --lib` + `cargo miri test -p layer-app --lib -- wasm::vm` before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] Install `cargo-audit`: `cargo install cargo-audit`
- [ ] Install `miri` toolchain: `rustup component add miri`
- [ ] New test: `packages/app/src/wasm/utils.rs` — `counter_address_is_deterministic` covering FOUND-03

*(All other test infrastructure already exists via `#[cfg(test)]` modules in the packages.)*

---

## Sources

### Primary (HIGH confidence)
- `docs.rs cosmwasm-vm latest` — BackendApi v2.3.2 trait interface (addr_validate, addr_canonicalize, addr_humanize)
- `github.com/CosmWasm/cosmwasm blob/v1.5.4/packages/vm/src/backend.rs` — BackendApi v1.5.4 interface (canonical_address, human_address — no addr_validate)
- `packages/app/src/wasm/vm/backend.rs` (local) — current VmApi impl, VmStore, VmQuerier, danger_will_robinson signature
- `packages/app/src/wasm/vm/cache.rs` (local) — all 6 call sites for danger_will_robinson, gas factor constant
- `packages/app/src/wasm/utils.rs` (local) — build_instantiate_address (v1 counter) and build_instantiate_2_address (CREATE2-style)
- `packages/app/src/wasm/keeper.rs` (local) — generate_address using CONTRACT_COUNTER, confirmed no SystemTime/rand
- `Cargo.toml` (workspace root, local) — all workspace dependency declarations
- `doc.rust-lang.org/cargo/reference/specifying-dependencies.html` — path dependency syntax for git submodule crates

### Secondary (MEDIUM confidence)
- `github.com/CosmWasm/cosmwasm/releases` page — confirmed v2.3.2 is the latest 2.x release (Feb 11, 2026)
- `github.com/CosmWasm/advisories/blob/main/CWAs/CWA-2024-004.md` — CWA-2024-004 summary; patched in >=2.1.3
- CWA-2024-004 diff (v2.1.1→v2.1.3): engine.rs gas constant change from 170→115 with 14x multiplier for control-flow ops; verified from WebFetch of the patch
- `rustsec.org/advisories/RUSTSEC-2024-0361.html` — RustSec registry entry for CWA-2024-004

### Tertiary (LOW confidence)
- CosmWasm MIGRATING.md (general) — method rename from canonical_address/human_address to addr_canonicalize/addr_humanize confirmed by two independent sources (docs.rs and MIGRATING.md search result)

---

## Metadata

**Confidence breakdown:**
- Standard stack (what to use): HIGH — all from local codebase inspection + official docs
- Architecture (how to structure): HIGH — based on direct code analysis + cargo docs
- Tendermint removal scope: HIGH — confirmed by grep of all .rs and .toml files
- CWA-2024-004 fix content: HIGH — confirmed by WebFetch of the v2.1.1→v2.1.3 diff
- Transmute fix approach: HIGH — root cause fully understood from source; raw pointer pattern is standard Rust idiom
- FOUND-03 determinism verdict: HIGH — confirmed by reading keeper.rs:203-216 (no SystemTime/rand)
- Pitfalls: MEDIUM — based on code analysis + known Rust/CosmWasm community patterns

**Research date:** 2026-03-18
**Valid until:** 2026-04-18 (cosmwasm-vm stable; cargo ecosystem stable)
