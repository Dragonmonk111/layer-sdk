# Phase 2: Commonware Consensus - Research

**Researched:** 2026-03-19
**Domain:** Commonware `threshold_simplex` consensus integration, BLS12-381 threshold certificates, WAL crash recovery, multi-node testnet, determinism enforcement
**Confidence:** MEDIUM (Commonware is ALPHA software at 2026.3.0; core trait API is confirmed via docs.rs; BLS DKG bootstrap is the key open question for a static validator set; threshold_simplex Config struct cannot be retrieved directly from docs.rs due to 404 on the sub-module page — API inferred from simplex module patterns and blog posts)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CONS-01 | CometBFT ABCI server replaced with Commonware `threshold_simplex` Automaton implementation | `Automaton` trait confirmed: `genesis`, `propose`, `verify` methods; `CertifiableAutomaton::certify` for finalization gating; `packages/abci/` and Cosmos/ABCI references deleted; slay3rd binary restructured around `LayerNode` struct |
| CONS-02 | `propose()`, `verify()`, `genesis()` wired to existing `App<T>` state machine | `App<T>` already has `finalize_block()`, `check_tx()`, `init()`, `query()` — maps directly; `propose()` pulls txs from application-managed mempool; `verify()` validates block against current state; finalization callback calls `finalize_block()` |
| CONS-03 | Supervisor trait for static validator set, extensible for dynamic | `Supervisor` is not a trait in `commonware-consensus` (as of 2026.3.0 — it was removed/renamed); participant set is managed through the `Scheme` type parameter's `participants()` method on the signing scheme; static set can be hardcoded at startup for Phase 2 |
| CONS-04 | State transitions are fully deterministic — no HashMap iteration, SystemTime, floats in certify/verify paths | Existing `App<T>` code uses `cosmwasm_std::Timestamp` for block time (from block header — safe); must audit for `HashMap`/`HashSet` iteration in `sm.rs`, `auth/keeper.rs`, `bank/keeper.rs`, `wasm/keeper.rs`; replace with `BTreeMap`/`BTreeSet`; CertifiableAutomaton::certify must be pure |
| CONS-05 | BLS12-381 threshold signature certificates per finalized block | threshold_simplex produces ~240-byte BLS certificates per finalized view; certificate stored in block header; verifiable offline with static shared public key from DKG; DKG must complete before node starts |
</phase_requirements>

---

## Summary

Phase 2 replaces the CometBFT ABCI server with Commonware `threshold_simplex` — a BFT consensus engine that natively embeds BLS12-381 threshold cryptography to produce succinct consensus certificates per block. The primary deliverable is `LayerNode` (a new struct in the slay3rd binary) that implements `CertifiableAutomaton` and drives the existing `App<T>` state machine through consensus callbacks.

The existing codebase is well-positioned for this migration. `App<T>` is already framework-agnostic — it exposes `init()`, `finalize_block()`, `check_tx()`, and `query()` as pure Rust methods with no ABCI bindings. The main additions are: (1) `LayerNode` wrapping `Arc<Mutex<App<T>>>` and implementing `CertifiableAutomaton`, (2) an application-managed mempool replacing the CometBFT mempool, (3) Commonware runtime/p2p/storage wiring in `slay3rd/src/main.rs`, and (4) BLS12-381 DKG bootstrapping for the threshold signing scheme.

The most significant risk in this phase is the BLS DKG bootstrap requirement. `threshold_simplex` requires validators to have run a DKG to establish a shared threshold secret before consensus can start. For the 3-node testnet required by CONS-02 success criteria, a simplified in-process DKG (using `commonware-cryptography::bls12381::dkg`) run at node startup is the right approach. The determinism requirement (CONS-04) requires an audit pass over all state machine code paths reachable from `certify()` and `verify()` to eliminate any `HashMap` iteration or wall-clock time usage.

**Primary recommendation:** Implement `LayerNode` as a `CertifiableAutomaton` in `slay3rd/`, wire it to `App<T>` via `Arc<Mutex<_>>`, run BLS DKG at startup from a static genesis validator set defined in config, and use `commonware-runtime::tokio::Runner` to drive the consensus engine alongside the existing gRPC server.

---

## Standard Stack

### Core (Phase 2 additions)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `commonware-consensus` | 2026.3.0 | BFT agreement with threshold_simplex; `CertifiableAutomaton` trait; block ordering | Purpose-built Rust consensus; only production-viable Rust BFT library in 2026 |
| `commonware-p2p` | 2026.3.0 | Authenticated P2P networking; peer registry by BLS/ED25519 public key | Required by consensus engine; provides `Blocker` for banning invalid peers |
| `commonware-cryptography` | 2026.3.0 | BLS12-381 DKG, threshold signing, Ed25519 node identity | Provides the signing scheme required by threshold_simplex |
| `commonware-runtime` | 2026.3.0 | Async task scheduling (deterministic for tests, tokio for production) | Swappable; `deterministic::Runner` enables reproducible tests |
| `commonware-storage` | 2026.3.0 | Append-only Journal (WAL) for consensus state; crash recovery | Provides `Journal` and `Persistable` trait; Voter syncs WAL before broadcast |

### Supporting (retained from Phase 1)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.28+ | Async runtime | Keep; Commonware's `tokio::Runner` wraps it |
| `tonic` | 0.12.3 | gRPC service layer | Keep; gRPC query/tx submission survives migration |
| `tracing` | 0.1.37 | Structured logging | Keep unchanged |
| `layer-storage` (rocksdb) | workspace | Application state persistence | Keep; App<T>'s PersistentStorage implementation |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `threshold_simplex` | `simplex` (Ed25519 multisig) | `simplex` has no threshold certs — cannot satisfy CONS-05; threshold_simplex is the only path to BLS certificates |
| Commonware `tokio::Runner` | Raw tokio | Raw tokio cannot swap in the `deterministic::Runner` for testing |
| Commonware WAL (Journal) | Custom RocksDB WAL | Commonware's Voter requires syncing its own WAL before broadcast; custom WAL does not satisfy the safety model |

**Installation:**
```toml
# In workspace Cargo.toml [workspace.dependencies]
commonware-consensus     = "2026.3.0"
commonware-p2p           = "2026.3.0"
commonware-cryptography  = "2026.3.0"
commonware-runtime       = "2026.3.0"
commonware-storage       = "2026.3.0"
```

**Version verification:** All five `commonware-*` crates confirmed at `2026.3.0` via crates.io API on 2026-03-19.

---

## Architecture Patterns

### Recommended Project Structure

After Phase 2, the `slay3rd` binary (formerly an ABCI app) becomes a Commonware node:

```
app/slay3rd/src/               (currently empty — ABCI binary was deleted in Phase 1)
├── main.rs                    # Runtime init; spawn Commonware + gRPC
├── node.rs                    # LayerNode: implements CertifiableAutomaton
├── mempool.rs                 # Application-managed tx mempool (simple VecDeque for Phase 2)
├── config.rs                  # Node config: validator keys, peers, listen addr, genesis
├── relay.rs                   # Implements Relay trait (broadcast payloads to peers)
├── block.rs                   # BlockPayload: serialization of tx batch + metadata
└── grpc/                      # Keep existing structure; types unchanged in Phase 2
    ├── mod.rs
    ├── tx.rs
    └── node.rs
packages/app/                  # UNCHANGED — App<T> is used as-is
packages/storage/              # UNCHANGED
```

The `packages/abci/` directory does not exist in the current workspace (it was removed in Phase 1-01). The workspace `Cargo.toml` already excludes `abci` from `members`. Verification: grep for `abci` in `Cargo.toml` confirms no abci crate member. The remaining cleanup is removing any residual proto references.

### Pattern 1: CertifiableAutomaton as Consensus Bridge

**What:** `LayerNode` wraps `Arc<Mutex<App<T>>>` and implements `CertifiableAutomaton`. It is the single integration point between Commonware consensus and the Layer state machine.

**When to use:** The only place where Commonware callbacks call into Layer application logic.

**Example:**
```rust
// Source: docs.rs/commonware-consensus (Automaton + CertifiableAutomaton traits)

use commonware_consensus::{Automaton, CertifiableAutomaton, Relay, Reporter};
use commonware_consensus::simplex::{Context, Round};   // types vary between simplex and threshold_simplex
use tokio::sync::oneshot;

pub struct LayerNode<T: PersistentStorage + Send + Sync + 'static> {
    app: Arc<Mutex<App<T>>>,
    mempool: Arc<Mutex<Mempool>>,
}

impl<T: PersistentStorage + Send + Sync + 'static> Automaton for LayerNode<T> {
    type Context = Context<[u8; 32], <BLS12381Scheme as Scheme>::PublicKey>;
    type Digest = [u8; 32];   // SHA-256 of serialized block payload

    async fn genesis(&mut self, _epoch: Epoch) -> Self::Digest {
        // Return initial app_hash from genesis state
        let app = self.app.lock().await;
        let hash: [u8; 32] = app.app_hash().try_into().expect("app_hash must be 32 bytes");
        hash
    }

    async fn propose(&mut self, _ctx: Self::Context) -> oneshot::Receiver<Self::Digest> {
        let (tx, rx) = oneshot::channel();
        // Pull txs from mempool, build BlockPayload, serialize, compute digest
        let txs = self.mempool.lock().await.drain_batch(MAX_BLOCK_TXS);
        let payload = BlockPayload { txs, timestamp: /* from ctx */ };
        let digest = sha256(&bincode::serialize(&payload).unwrap());
        // Store payload in pending map keyed by digest
        tx.send(digest).ok();
        rx
    }

    async fn verify(&mut self, _ctx: Self::Context, digest: Self::Digest) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        // Fetch payload by digest (from relay or local cache), basic sanity checks
        // Do NOT execute txs here — only structural validation
        let valid = self.validate_payload(digest).await;
        tx.send(valid).ok();
        rx
    }
}

impl<T: PersistentStorage + Send + Sync + 'static> CertifiableAutomaton for LayerNode<T> {
    async fn certify(&mut self, _round: Round, digest: Self::Digest) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        // DETERMINISM CRITICAL: no HashMap iteration, no SystemTime, no floats
        // Apply block to App<T> here — finalize_block() is called from certify
        let result = self.execute_block(digest).await;
        tx.send(result.is_ok()).ok();
        rx
    }
}
```

**IMPORTANT:** `certify()` is where `App::finalize_block()` is called. Once `certify()` returns `true`, the block is committed. The block payload must have been fetched and decoded before `certify()` is called. The default `certify()` returns `true` immediately — override is required to execute state.

### Pattern 2: Mempool (Application-Managed)

**What:** Commonware provides no mempool. The application manages a simple queue.

**When to use:** Between gRPC tx submission and consensus `propose()`.

**Example:**
```rust
// Simple Phase 2 mempool — no priority, no eviction, just FIFO
pub struct Mempool {
    queue: VecDeque<Tx>,
    max_pending: usize,
}

impl Mempool {
    pub fn submit(&mut self, tx: Tx) -> bool {
        if self.queue.len() >= self.max_pending { return false; }
        self.queue.push_back(tx);
        true
    }

    pub fn drain_batch(&mut self, max: usize) -> Vec<Tx> {
        (0..max).filter_map(|_| self.queue.pop_front()).collect()
    }
}
```

### Pattern 3: BlockPayload Encoding

**What:** The consensus engine operates on opaque binary digests. Block format is entirely application-defined. Phase 2 must define a wire format for block payloads.

**When to use:** In `propose()` to encode, in `verify()`/`certify()` to decode.

**Example:**
```rust
// Simple binary encoding — avoid JSON for determinism; use bincode or a custom format
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BlockPayload {
    pub height: u64,
    pub timestamp_nanos: u64,       // from consensus context — NOT SystemTime::now()
    pub proposer: Vec<u8>,          // proposer's public key bytes
    pub txs: Vec<Bytes>,            // raw transaction bytes
    pub parent_digest: [u8; 32],    // digest of parent block's payload
}
```

**Determinism rule:** The serialization of `BlockPayload` must be deterministic. `bincode` with its default configuration is deterministic for structs with fixed field order. `serde_json` is NOT recommended — float handling and map key ordering can vary.

### Pattern 4: BLS DKG Bootstrap for Static Validator Set

**What:** `threshold_simplex` requires validators to have a shared BLS12-381 threshold secret before starting. For a static 3-node testnet (Phase 2), the simplest approach is to run an in-process DKG among the 3 nodes at startup using direct authenticated connections.

**When to use:** Node startup, before spawning the consensus engine.

**Example:**
```rust
// Phase 2: simplified DKG for static 3-node testnet
// Each node's Ed25519 identity key is used for authenticated P2P
// DKG runs over commonware-p2p authenticated channels

// Option A: offline keygen (simpler for testnet)
// - Use commonware-cryptography::bls12381::dkg offline tooling
// - Each validator has: (bls_secret_share, bls_public_share, threshold_public_key)
// - Keys written to node config file at testnet setup time

// Option B: in-process DKG at startup
// - Nodes connect via commonware-p2p
// - Run bls12381::dkg::generate() once at epoch 0
// - Each node stores its share to disk; shared public key is known to all
```

**ALPHA caveat:** The DKG API in `commonware-cryptography 2026.3.0` is `ALPHA` stability. Expect API changes. Pin the exact version in `Cargo.lock`.

### Pattern 5: WAL and Crash Recovery

**What:** Commonware's Voter (internal consensus engine component) requires its WAL to be synced to disk before broadcasting votes. This prevents Byzantine behavior after unclean shutdowns.

**When to use:** Always — it is not optional.

**Implementation notes:**
- Use `commonware-storage::Journal` as the WAL backend for the Voter
- Do NOT implement custom async-flushed WAL
- The `commonware-storage::Persistable` trait handles recovery: `load()` restores state across restarts
- The Resolver component fetches missing historical artifacts on restart — nodes automatically catch up to the current view

```rust
// Crash recovery: the Voter restores from WAL automatically
// The application's App<T> loads from RocksDB on startup:

let mut app = App::new(storage, logic);
match app.load_from_storage() {
    Ok(()) => { /* continue from where we left off */ }
    Err(AppLoadError::NoStoredState) => {
        // Genesis: get genesis config and call app.init()
    }
    Err(e) => panic!("State corruption: {e}"),
}
```

For CONS-03 (crash-and-rejoin), the Resolver component automatically fetches blocks the node missed while offline. The application's `verify()` must be able to accept blocks out of chronological order during catchup.

### Anti-Patterns to Avoid

- **Porting ABCI 1:1:** Do not map `check_tx → mempool`, `finalize_block → certify` as direct ports. Commonware is a lower-level primitive. Design `LayerNode` from scratch as a state machine driver.
- **Calling finalize_block from `verify()`:** `verify()` is called for all received proposals before consensus — it must not mutate state. Only `certify()` commits state.
- **Using SystemTime in block metadata:** Block timestamps must come from the consensus context (`ctx.timestamp` or equivalent), not `SystemTime::now()`. The context value is the same on all validators for a given view.
- **HashMap in certify() or any code path it calls:** HashMap iteration order is non-deterministic in Rust. Replace all `HashMap`/`HashSet` in consensus-critical paths with `BTreeMap`/`BTreeSet`.
- **Async WAL flush:** Never configure async WAL flushing for the Voter's underlying storage. Synchronous WAL sync before broadcast is mandatory for safety.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| BFT agreement | Custom PBFT/Tendermint port | `commonware-consensus::threshold_simplex` | Fork of decades-old protocol; cryptographic proofs of safety/liveness are non-trivial |
| Threshold BLS signatures | Custom BLS12-381 | `commonware-cryptography::bls12381` | Threshold signature schemes have subtle security requirements (DKG correctness, resharing) |
| P2P networking | Custom libp2p setup | `commonware-p2p` | Authenticated transport, Blocker for Byzantine peer management, lazy batch verification |
| Write-ahead log | Custom RocksDB WAL | `commonware-storage::Journal` | Voter requires WAL sync before broadcast; Journal's Persistable trait handles recovery |
| Leader election | Custom round-robin | `commonware-consensus` Elector | Elector is the consensus-defined leader selection; using anything else breaks safety |
| Tx serialization | Custom JSON encoding | `bincode` (deterministic) | JSON has non-deterministic float and map key handling; bincode produces identical bytes for identical inputs |

**Key insight:** Commonware's value is that it provides cryptographically correct primitives. The application's job is wiring them together and implementing the state machine logic. Do not implement consensus from scratch.

---

## Common Pitfalls

### Pitfall 1: Non-Determinism in certify() Halts the Chain

**What goes wrong:** `certify()` must return the same `bool` on every honest validator for a given `(round, digest)` pair. Any code path reachable from `certify()` that uses `HashMap` iteration, `SystemTime`, or floating-point arithmetic will cause different validators to return different results, preventing the 2f+1 finalize votes needed to finalize the view.

**Why it happens:** Developers port existing `finalize_block()` logic directly into `certify()` without auditing for non-determinism sources.

**How to avoid:**
- Replace all `HashMap`/`HashSet` in `sm.rs`, `auth/keeper.rs`, `bank/keeper.rs`, `wasm/keeper.rs` with `BTreeMap`/`BTreeSet` before Phase 2 begins
- Block timestamps in `certify()` must come from the block payload, not `SystemTime::now()`
- Run the same block through `certify()` twice with different process entropy and assert identical result

**Warning signs:** Different nodes produce different `AppHash` for the same finalized block height. The chain stalls at a specific height. Tests pass locally but fail in CI.

### Pitfall 2: WAL Not Synced Before Broadcast

**What goes wrong:** If the Voter broadcasts a vote before persisting it to WAL, a crash between broadcast and WAL sync causes the node to miss the vote on restart and potentially re-broadcast a conflicting message — Byzantine behavior even if unintentional.

**Why it happens:** Developers add async flushing to improve throughput.

**How to avoid:** Use `commonware-storage::Journal` exactly as documented. Do not wrap it with async buffering. The Voter handles WAL sync internally when configured with the standard Journal.

**Warning signs:** Custom async flush code around the consensus Journal. Any performance optimization touching the WAL path.

### Pitfall 3: DKG Not Completed Before Consensus Starts

**What goes wrong:** `threshold_simplex` cannot produce threshold signatures without a shared BLS secret. If nodes try to start consensus before DKG completes, there is no threshold public key to verify certificates against.

**Why it happens:** DKG is a separate pre-consensus step that is easy to skip during development.

**How to avoid:** For the Phase 2 testnet, either (a) run an offline DKG ceremony and distribute keys via config files, or (b) implement a startup DKG step that blocks node startup until completion. The node must not start consensus until the DKG result (each validator's share + the shared public key) is persisted to disk.

**Warning signs:** Consensus starts but never produces certificates. Nodes report missing threshold key material. The `CertifiableAutomaton::certify()` can return `true` without producing a certificate if the signing scheme is not properly initialized.

### Pitfall 4: verify() Mutates State

**What goes wrong:** `verify()` is called for every received proposal, including proposals from Byzantine or slow nodes. If `verify()` mutates `App<T>` state, the application state becomes corrupted before consensus finalizes any block.

**Why it happens:** Confusion about the Commonware model: `verify()` is structural validation only; `certify()` is where state execution belongs.

**How to avoid:** In `verify()`, decode the block payload and check format validity only. Do not call `app.finalize_block()` or any state-mutating method. Call `finalize_block()` only from `certify()`.

### Pitfall 5: Cargo Dependency Conflicts with Commonware

**What goes wrong:** Commonware 2026.3.0 depends on specific versions of `tokio`, `bytes`, `prost`, and `ring`. The existing workspace already pins `bytes = "1.11.1"` and `tokio = "1.28.0"`. Adding Commonware may create duplicate major version entries.

**How to avoid:**
- Run `cargo tree -d` immediately after adding Commonware dependencies
- If conflicts appear, use `[patch.crates-io]` in the workspace `Cargo.toml`
- Commonware 2026.3.0 uses `tokio 1.x` — no major version conflict expected
- Watch for `ring` and `rustls` conflicts if Commonware's P2P uses different TLS backends

---

## Code Examples

Verified patterns from official sources:

### Automaton Trait Definition (docs.rs/commonware-consensus)
```rust
// Source: docs.rs/commonware-consensus/latest/commonware_consensus/trait.Automaton.html
pub trait Automaton: Clone + Send + 'static {
    type Context;
    type Digest: Digest;

    fn genesis(
        &mut self,
        epoch: Epoch,
    ) -> impl Future<Output = Self::Digest> + Send;

    fn propose(
        &mut self,
        context: Self::Context,
    ) -> impl Future<Output = oneshot::Receiver<Self::Digest>> + Send;

    fn verify(
        &mut self,
        context: Self::Context,
        payload: Self::Digest,
    ) -> impl Future<Output = oneshot::Receiver<bool>> + Send;
}
```

### CertifiableAutomaton Trait Definition (docs.rs/commonware-consensus)
```rust
// Source: docs.rs/commonware-consensus/latest/commonware_consensus/trait.CertifiableAutomaton.html
// Extends Automaton; default certify() returns true immediately
pub trait CertifiableAutomaton: Automaton {
    fn certify(
        &mut self,
        _round: Round,
        _payload: Self::Digest,
    ) -> impl Future<Output = oneshot::Receiver<bool>> + Send {
        // Default: always certify immediately
        async move {
            let (sender, receiver) = oneshot::channel();
            sender.send_lossy(true);
            receiver
        }
    }
}
```

### Relay Trait (docs.rs/commonware-consensus)
```rust
// Source: docs.rs/commonware-consensus/latest/commonware_consensus/trait.Relay.html
pub trait Relay: Clone + Send + 'static {
    type Digest: Digest;
    type PublicKey: PublicKey;
    type Plan: Send;

    fn broadcast(
        &mut self,
        payload: Self::Digest,
        plan: Self::Plan,
    ) -> impl Future<Output = ()> + Send;
}
```

### Reporter Trait (docs.rs/commonware-consensus)
```rust
// Source: docs.rs/commonware-consensus/latest/commonware_consensus/trait.Reporter.html
pub trait Reporter: Clone + Send + 'static {
    type Activity;
    fn report(&mut self, activity: Self::Activity) -> impl Future<Output = ()> + Send;
}
```

### App<T> finalize_block Integration
```rust
// Source: packages/app/src/app.rs (existing code)
// finalize_block() is called from certify() in LayerNode:
impl<T: PersistentStorage + 'static> LayerNode<T> {
    async fn execute_block(&mut self, digest: [u8; 32]) -> Result<(), AppError> {
        let payload = self.pending_payloads
            .remove(&digest)
            .ok_or(AppError::MissingPayload)?;

        let block = Block {
            txs: payload.txs,
            height: payload.height,
            time: Timestamp::from_nanos(payload.timestamp_nanos),
            proposer_address: payload.proposer,
            last_votes: vec![],
        };

        let mut app = self.app.lock().await;
        app.finalize_block(block)?;
        Ok(())
    }
}
```

### BLS12-381 Threshold Simplex Config Structure
```rust
// Source: docs.rs/commonware-consensus/latest/src/commonware_consensus/simplex/config.rs.html
// (threshold_simplex has equivalent Config — adapt as needed)
// Type parameters from simplex::Config as reference:
pub struct Config<S, L, B, D, A, R, F, T>
where
    S: Scheme,                        // BLS12-381 signing scheme with DKG shares
    L: Elector<S>,                    // Leader election (round-robin or VRF)
    B: Blocker<PublicKey = S::PublicKey>,  // P2P blocker
    D: Digest,                        // [u8; 32] for SHA-256
    A: CertifiableAutomaton<Context = Context<D, S::PublicKey>>,  // LayerNode
    R: Relay,                         // payload broadcast
    F: Reporter<Activity = Activity<S, D>>,  // consensus activity reporting
    T: Strategy,                      // parallel operation strategy
{
    pub scheme: S,
    pub elector: L,
    pub blocker: B,
    pub automaton: A,
    pub relay: R,
    pub reporter: F,
    pub strategy: T,
    pub partition: String,
    pub mailbox_size: usize,
    pub epoch: Epoch,
    pub replay_buffer: NonZeroUsize,
    pub write_buffer: NonZeroUsize,
    pub page_cache: CacheRef,
    pub leader_timeout: Duration,
    pub certification_timeout: Duration,
    pub timeout_retry: Duration,
    pub activity_timeout: ViewDelta,
    pub skip_timeout: ViewDelta,
    pub fetch_timeout: Duration,
    pub fetch_concurrent: usize,
    pub forwarding: ForwardingPolicy,
}
```

### Determinism-Safe Block Timestamp Pattern
```rust
// NEVER do this in certify() or verify():
let bad_time = std::time::SystemTime::now();  // NON-DETERMINISTIC

// Do this instead — use timestamp from block payload which came from consensus context:
let timestamp_nanos: u64 = block_payload.timestamp_nanos;  // from propose() ctx, same on all validators
let block_time = cosmwasm_std::Timestamp::from_nanos(timestamp_nanos);
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| CometBFT ABCI socket server | Commonware `Automaton` in-process trait | Phase 2 | No more IPC; consensus calls directly into application Rust code |
| ABCI `FinalizeBlock` callback | `CertifiableAutomaton::certify()` | Phase 2 | Application controls when to commit; finalization is asynchronous notification |
| CometBFT P2P gossip | `commonware-p2p` authenticated transport | Phase 2 | Identity-based P2P; peers authenticated by public key; Byzantine peers blocked automatically |
| Ed25519 multisig block certificates | BLS12-381 threshold certificates | Phase 2 | ~240 bytes per certificate; verifiable with static public key; prerequisite for zkVM rollup |
| CometBFT-managed mempool | Application-managed `VecDeque` mempool | Phase 2 | Application controls tx ordering and selection entirely |
| CometBFT `check_tx` filter | Application `App::check_tx()` in gRPC submit handler | Phase 2 | `check_tx()` is still called before mempool insertion; no behavior change |

**Deprecated/outdated (from this codebase):**
- `packages/abci/`: Deleted in Phase 1. No abci package in workspace. Confirmed via `Cargo.toml` grep.
- Tendermint/CometBFT proto types in `packages/proto/`: Still present but unused after Phase 2; defer deletion to Phase 3
- `BlockInfo` from cosmwasm_std: Still used inside `App<T>`; safe to keep through Phase 2 since the state machine still uses Cosmos types per the roadmap decision

---

## Open Questions

1. **threshold_simplex Config exact API**
   - What we know: The `simplex::Config` struct is confirmed with full field list. `threshold_simplex` is a separate module at `commonware_consensus::threshold_simplex` — it likely has an analogous Config struct.
   - What's unclear: The docs.rs page for `threshold_simplex/index.html` returns 404 as of 2026-03-19. The module may have been reorganized. Source code on GitHub shows `consensus/src/` contains only `simplex/` subdirectory — threshold_simplex may be in a scheme variant under `simplex/scheme/`.
   - Recommendation: At plan time, inspect `cargo doc --open` with `commonware-consensus = "2026.3.0"` to get the live API. The plan should confirm whether `threshold_simplex` is a separate module or a scheme variant of `simplex`. If it's a scheme, the Config is the same as `simplex::Config` with a different `S: Scheme` type parameter.

2. **DKG bootstrap for 3-node testnet**
   - What we know: Threshold_simplex requires pre-completed DKG. `commonware-cryptography::bls12381::dkg` provides the implementation. DKG and resharing are decoupled from consensus itself.
   - What's unclear: The exact crate API for initiating a 3-party DKG — whether it's synchronous (useful for testnet genesis) or requires the P2P layer.
   - Recommendation: Plan Wave 0 should include a `generate_keys` binary (or test helper) that runs a 3-of-3 BLS DKG offline and writes the result to config files for each node. This avoids blocking consensus implementation on DKG networking.

3. **threshold_simplex certificate structure**
   - What we know: Each finalized view produces a ~240-byte BLS12-381 threshold signature certificate. It is verifiable with a single BLS signature check using the static shared public key.
   - What's unclear: The exact Rust type for the certificate (struct name, fields). How to serialize it for storage in the block header.
   - Recommendation: The plan should include a task to inspect the certificate type from `commonware-cryptography` and define a `BlockCertificate` wrapper in `layer-std` for block header storage.

4. **App<T> height tracking with Commonware views**
   - What we know: App<T> tracks `LAST_BLOCK` using `BlockInfo::height` (u64). Commonware uses `View` (u64) and `Height` (u64) as separate concepts — a view is a consensus round, multiple views may correspond to the same block height if a view is nullified.
   - What's unclear: Whether Layer maps one Commonware view = one block height, or uses height = number of committed (non-nullified) views.
   - Recommendation: Map committed views (successful finalize certificates) to Layer block heights. Nullified views do not increment the block height. This preserves App<T>'s sequential height assumption.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `cargo test` |
| Config file | none (workspace-level) |
| Quick run command | `cargo test -p layer-app -- --test-thread=1` |
| Full suite command | `cargo test --workspace -- --test-threads=4` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CONS-01 | slay3rd binary starts with LayerNode (no CometBFT imports) | build + smoke | `cargo build --bin slay3rd && cargo test -p layer-app` | ❌ Wave 0 |
| CONS-01 | No `tendermint` or `cometbft` crates in workspace deps | dependency audit | `cargo tree \| grep -E 'tendermint\|cometbft'` | ❌ Wave 0 |
| CONS-02 | 3-node testnet reaches consensus, identical AppHash per block | integration | manual testnet + script comparison | ❌ Wave 0 |
| CONS-02 | propose/verify/certify callbacks call App<T> correctly | unit | `cargo test -p slay3rd` | ❌ Wave 0 |
| CONS-03 | Node restarts from WAL and rejoins testnet | integration | crash-recovery test script | ❌ Wave 0 |
| CONS-04 | No HashMap iteration in certify/verify paths | audit + unit | `cargo test -p layer-app -- determinism` | ❌ Wave 0 |
| CONS-04 | Same block processed twice produces identical AppHash | regression | `cargo test -p layer-app -- app::tests::deterministic_finalize` | ❌ Wave 0 |
| CONS-05 | BLS certificate produced per finalized block | unit | `cargo test -p slay3rd -- certificate` | ❌ Wave 0 |
| CONS-05 | Certificate verifiable by offline verifier | unit | `cargo test -p slay3rd -- verify_certificate_offline` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cargo test -p layer-app -- --test-threads=1`
- **Per wave merge:** `cargo test --workspace -- --test-threads=4`
- **Phase gate:** Full suite green + 3-node testnet consensus demonstration before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `app/slay3rd/src/` — currently empty (Phase 1 deleted old ABCI binary); needs new `main.rs`, `node.rs`, `mempool.rs`, `relay.rs`, `block.rs`
- [ ] `app/slay3rd/Cargo.toml` — needs to be created with commonware-* dependencies
- [ ] `tests/determinism_test.rs` — regression test running same block twice and asserting identical AppHash
- [ ] `tests/no_tendermint_deps.rs` OR CI check — `cargo tree | grep tendermint` must return empty
- [ ] Key generation tooling: `tools/generate-testnet-keys/` — offline BLS DKG for 3-node genesis

*(Note: `app/slay3rd/` directory exists but is empty; it was cleared when the ABCI binary was deleted in Phase 1-01. The directory needs to be repopulated from scratch.)*

---

## Sources

### Primary (HIGH confidence)
- [docs.rs/commonware-consensus — Automaton trait](https://docs.rs/commonware-consensus/latest/commonware_consensus/trait.Automaton.html) — `genesis`, `propose`, `verify` method signatures confirmed
- [docs.rs/commonware-consensus — CertifiableAutomaton trait](https://docs.rs/commonware-consensus/latest/commonware_consensus/trait.CertifiableAutomaton.html) — `certify` signature and determinism requirement confirmed
- [docs.rs/commonware-consensus — Relay trait](https://docs.rs/commonware-consensus/latest/commonware_consensus/trait.Relay.html) — `broadcast` signature confirmed
- [docs.rs/commonware-consensus — Reporter trait](https://docs.rs/commonware-consensus/latest/commonware_consensus/trait.Reporter.html) — `report` signature confirmed
- [docs.rs/commonware-consensus — simplex Config struct](https://docs.rs/commonware-consensus/latest/src/commonware_consensus/simplex/config.rs.html) — full field list confirmed
- [docs.rs/commonware-storage](https://docs.rs/commonware-storage/latest/commonware_storage/) — Journal, Persistable, crash-recovery primitives
- [crates.io API — version verification](https://crates.io/api/v1/crates/commonware-consensus) — 2026.3.0 confirmed latest for all 5 commonware-* crates on 2026-03-19
- Layer SDK codebase — `packages/app/src/app.rs` — `App<T>` interface confirmed (direct read)
- Layer SDK codebase — `packages/app/src/sm.rs` — `StateMachine` module structure confirmed (direct read)
- Layer SDK workspace `Cargo.toml` — confirmed no `abci` workspace member, no Tendermint deps

### Secondary (MEDIUM confidence)
- [Commonware threshold-simplex blog](https://commonware.xyz/blogs/threshold-simplex) — BLS DKG requirement, ~240-byte certificate structure, resharing for validator reconfiguration
- [docs.rs/commonware-consensus — lib.rs source](https://github.com/commonwarexyz/monorepo/blob/main/consensus/src/lib.rs) — module structure: simplex (BETA), aggregation, marshal, ordered_broadcast, types
- Prior Phase 1 research `.planning/research/STACK.md` — Commonware integration pattern, crate versions
- Prior Phase 1 research `.planning/research/ARCHITECTURE.md` — LayerNode pattern, Automaton-as-bridge design
- Prior Phase 1 research `.planning/research/PITFALLS.md` — consensus non-determinism (HashMap), WAL sync requirement, DKG pitfall
- [commonwarexyz/monorepo GitHub — consensus dir](https://github.com/commonwarexyz/monorepo/tree/main/consensus) — confirmed source structure: `simplex/`, `aggregation/`, no separate `threshold_simplex/` directory (may be scheme variant)

### Tertiary (LOW confidence)
- [commonware-consensus docs.rs — threshold_simplex index](https://docs.rs/commonware-consensus/latest/commonware_consensus/threshold_simplex/index.html) — page returns 404; module structure inferred from simplex patterns and blog posts
- [docs.rs/commonware-consensus — simplex/scheme/mod.rs](https://docs.rs/commonware-consensus/latest/src/commonware_consensus/simplex/scheme/mod.rs.html) — threshold_simplex may be a signing scheme variant of simplex (BLS12381 threshold scheme) rather than a top-level module

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all 5 commonware-* crates confirmed at 2026.3.0 via crates.io API
- Automaton/CertifiableAutomaton trait API: HIGH — confirmed via docs.rs with exact method signatures
- threshold_simplex module location: LOW — docs.rs 404; inferred it is likely a scheme variant under simplex::Config<BLS12381ThresholdScheme>
- BLS DKG bootstrap API: MEDIUM — existence confirmed via blog and cryptography docs; exact function signatures not verified
- Architecture patterns (LayerNode, mempool, WAL): MEDIUM — inferred from official docs + prior research; no executed code yet

**Research date:** 2026-03-19
**Valid until:** 2026-04-18 (30 days; Commonware is ALPHA — re-verify if a new minor version ships)
