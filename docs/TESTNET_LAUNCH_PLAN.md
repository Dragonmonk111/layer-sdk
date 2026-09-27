# JunoClaw Testnet Launch Plan

*Drafted 2026-09-27. Living document — tick items in place, don't fork copies.*

This plan is written against the code as it is, not as we'd like it to be.
Every "today" statement below was checked against source; file references are
given so anyone can re-verify.

---

## 0. Where we actually are

| Area | Today | Source |
|---|---|---|
| Consensus | Commonware simplex, threshold BLS12-381 certificates, round-robin leader | `app/slay3rd/src/main.rs`, `node.rs` |
| Block production | Proposer drains ≤100 txs FIFO from its **local** mempool | `node.rs` `propose()`, `MAX_BLOCK_TXS` |
| Proposal validation | `verify()` only checks the payload **exists** locally (by digest) | `node.rs` `verify()` |
| Payload relay | Content-addressed (digest recomputed on receipt) — good | `relay.rs` `receive_payload()` |
| Block timestamp | `genesis_time + view × 1s` (deterministic, not wall-clock) | `node.rs` `propose()` |
| Block gas | `params.max_gas`, **infinite if unset** | `packages/app/src/app.rs` `block_gas_meter()` |
| Mempool | FIFO `VecDeque`, cap 10 000, no gossip, no dedup, no recheck | `mempool.rs`, `config.rs` |
| Fee floor | `min_gas_price` enforced in `check_tx` (validator-local) | `config.rs` |
| Tx auth | secp256k1 only | `packages/cosmos/src/tx.rs` |
| PQ crypto | In-contract only (`jclaw-credential`): MAYO-2/3/5 verified on devnet | article 2026-09-26 |
| Tx queries | `BroadcastTx` ✔, `GetTx` ✔ (in-memory, this commit), `Simulate` ✘, `GetTxsEvent` ✘ | `grpc.rs` |
| IBC | BLS light client on one counterparty, one channel | `BLS_LIGHT_CLIENT_SPEC.md` |

## 1. Gates

Nothing moves to the next gate until **every** item in the current gate is
ticked and its verification artifact (test name, log, tx hash) is linked.

- **G0 — Devnet hardened** (us only). Consensus/mempool correctness, query surface, gas caps.
- **G1 — Closed testnet** (3–5 invited external validators). Genesis ceremony, soak, monitoring.
- **G2 — Public testnet.** Faucet, explorer, docs, fuzz + external review of the PQ verifier done.

---

## 2. Consensus & block production (G0 — highest priority)

The chain's biggest risk is not crypto; it is that **`verify()` trusts the
proposer**. Any validator that becomes leader can propose a payload with a
wrong height, wrong parent, wrong `state_root`, a timestamp in the far
future, or 10 000 txs, and honest validators will vote for it. With a small
trusted devnet this never bit us. With strangers it will.

| # | Task | Decision | Verification |
|---|---|---|---|
| C1 | **Validate payloads in `verify()`** | Reject unless: `height == current+1`; `parent_digest == last_digest`; `state_root == local state_root()`; `timestamp > parent.timestamp`; `txs.len() ≤ MAX_BLOCK_TXS`; total bytes ≤ `MAX_BLOCK_BYTES`; every tx parses with `parse_cosmos_tx`. Pure, read-only — no state mutation in `verify()`. | Unit test per rejection rule + 4-node devnet with one patched byzantine proposer: chain must skip its views, never finalize its blocks. |
| C2 | **Timestamps** | Keep view-derived time for G0 (deterministic, already safe). Before G1 switch to proposer wall-clock bounded by validators: `parent < t ≤ local_now + 2s` (CometBFT PBTS idea, simplified). Contracts reading `env.block.time` need real time. | Drift test: 1 000 blocks, `|block_time − wall_clock| < 5s`. |
| C3 | **Block gas cap** | Set `max_gas` in genesis consensus params. Start at **75 000 000**: 100 txs × worst measured honest tx (MAYO-5 verify, 726k) ≈ 72.6M. Revisit after C7 benchmark. | Genesis test asserts `max_gas.is_some()`; overflow test: block of 101 × 726k-gas txs splits across blocks. |
| C4 | **Block byte cap** | `MAX_BLOCK_BYTES = 8 MiB` (store-code txs are ~4.4 MB; CometBFT's default is 21 MiB — we're deliberately tighter). | Oversize proposal rejected by C1. |
| C5 | **Execute-on-certify failure** | If `finalize_block` errors after notarization the node now logs and returns `false` — a divergence. Decide: **halt the node with a clear error** (determinism bug = stop, don't fork). | Fault-injection test: forced finalize error → process exits non-zero, peers continue. |
| C6 | **Bound `pending_payloads`** | Peers can fill it with valid-but-useless payloads. Cap by height window (drop entries `< current_height`), max N entries. | Unit test on eviction. |
| C7 | **Throughput benchmark** | Measure block exec time for 100 × {bank send, MAYO-2, MAYO-5}. Target: p99 block exec < 50% of `leader_timeout_ms`. | Bench numbers recorded here. |
| C8 | **Validator set changes** | G1: static set, changed only by coordinated genesis/restart. G2: document the upgrade path (no `x/upgrade` equivalent exists — binary swaps are manual). | `VALIDATOR_ONBOARDING.md` dry-run by someone who didn't write it. |
| C9 | **Multi-node soak** | ≥4 validators, ≥24h, with leader kill, network partition (1 node), restart-with-recreate, and the C1 byzantine proposer. | Soak log + finalized-height graph attached. |

## 3. Mempool (G0)

Today a tx submitted to node A only lands when **A** is leader. With N
validators in round-robin that is up to N views of latency, and if A is down
the tx is simply gone.

| # | Task | Decision |
|---|---|---|
| M1 | **Tx propagation** | **Leader-forwarding, not full gossip.** The elector is `RoundRobin` — the next leaders are known. On `BroadcastTx`, forward the tx over the authenticated P2P channel to the next 2 leaders (Solana's Gulf Stream idea, which fits our known schedule). Cheaper than CometBFT flood-gossip and adequate for ≤20 validators. Revisit if the set grows. |
| M2 | **Dedup** | Seen-cache keyed by `sha256(tx_bytes)` (same hash as `GetTx`), TTL ~10 min. Drop duplicates at submit. |
| M3 | **Recheck after commit** | After each `finalize_block`, re-run `check_tx` on remaining mempool txs; drop stale-sequence / now-invalid ones (CometBFT does this by default). |
| M4 | **Sender ordering** | Within a batch, order by (sender, sequence) so a sender's txs 5,6,7 aren't included as 6,5,7 and fail. |
| M5 | **Gas-aware drain** | `drain_batch` stops at `max_gas` using each tx's `gas_wanted`, not just count. |
| M6 | **Don't lose drained txs** | If a view times out after `propose()` drained the pool, re-insert that payload's txs at the front. |
| M7 | **Limits** | `max_tx_bytes = 1 MiB` (CometBFT default) **except** `MsgStoreCode`, allowed up to 5 MiB; per-sender pending cap (e.g. 64). `min_gas_price` already exists. |
| M8 | **Priority (G2, optional)** | Fee-priority ordering only if spam shows up. FIFO + fee floor is enough for a testnet. |

## 4. Query / API surface

| # | Task | Gate | Status |
|---|---|---|---|
| Q1 | `GetTx` (height, code, raw_log, gas, events) | G0 | ✔ in-memory index, 100k window — `tx_index.rs` |
| Q2 | Persist tx index to RocksDB (survives restart) | G1 | ☐ |
| Q3 | Return decoded `tx` body in `GetTx` (needs `prost` decode of `TxRaw`) | G1 | ☐ |
| Q4 | `Simulate` (gas estimation — wallets need it) | G1 | ☐ (`App::simulate_gas_meter` already exists) |
| Q5 | `GetTxsEvent` (search by event) | G2 | ☐ |
| Q6 | `BroadcastTx` response carries `txhash` | G0 | ☐ (relayer computes locally today) |
| Q7 | Relayer `jc-tx <hash>` subcommand + poll-after-broadcast | G0 | ☐ |
| Q8 | CometBFT-RPC compatibility for cosmjs/Keplr — verify what `gateway/` covers | G1 | ☐ |

## 5. Wasm & gas safety (G0)

- **W1** Per-tx gas cap enforced (already via `gas_wanted`), plus block cap (C3).
- **W2** Metering audit: craft worst-case inputs (max-length MAYO pk/sig, deep JSON, huge `Bud` trees) and record gas + wall time. Criterion: wall time / gas ratio within 2× of honest path.
- **W3** Wasm code size limit. Devnet stores ~4.4 MB wasm; wasmd's default `MaxWasmSize` is 800 KiB. Pick a limit explicitly (proposal: 5 MiB) and enforce it.
- **W4** Memory limit per instance and query gas limit — confirm values in the vendored VM and document them.

---

## 6. Deterministic PQC settlement plan

"Deterministic" here means: **parameters are chosen now, each phase has a
fixed exit test, and nothing is left to be decided under launch pressure.**

### Fixed choices

| Use | Scheme | Why |
|---|---|---|
| Account / tx auth | **ML-DSA-65** (FIPS 204) | NIST standard (Aug 2024), category 3, broad library support. |
| Compact attestations (credential graph) | **MAYO-2 / MAYO-5** | Small signatures, already live on devnet. MAYO is a candidate in NIST's additional-signatures process, **not yet a standard** — keep it for attestations, not for money. |
| Transition mode | **Hybrid secp256k1 + ML-DSA-65** accounts | Security ≥ the stronger of the two; existing wallets keep working. |
| PQ finality | Per-validator **ML-DSA-65** signatures over finalized checkpoints | BLS aggregation has no practical PQ drop-in; individual signatures are simple and auditable. |
| PQ proof compression (later) | Lattice SNARK (Jolt + Akita) over the checkpoint signatures | See §7. |

### Phases

| Phase | Deliverable | Exit test | Gate |
|---|---|---|---|
| **PQ-0** ✔ | In-contract MAYO-2/3/5 verify | Live txs at heights 460 286 – 462 574, tampered sig rejected | done |
| **PQ-1** | Harden in-contract verifiers | NIST KATs for MAYO-1/2/3/5 + ML-DSA-44/65/87 pass on devnet; 24h `cargo fuzz` on each verifier, pk-hash path and `Bud` with zero crashes; length caps on pk/sig inputs | G1 |
| **PQ-2** | Native verifier host function in the vendored VM (`mayo-precompile` flag already in contract) | Fixed gas schedule `base + per_byte`; bit-identical results x86_64 vs aarch64 on all KATs; contract interface unchanged | G2 |
| **PQ-3** | Hybrid PQ accounts (new `PubKey` variant, ante handler checks both sigs) | Tx signed with secp256k1 + ML-DSA-65 accepted; either signature missing/invalid → rejected; gas charged per ML-DSA verify | post-G2 |
| **PQ-4** | PQ finality checkpoints: every K blocks, validators ML-DSA-65-sign `(height, block_digest, state_root)`; ≥2/3 stored alongside the BLS cert | Light-client test verifies a checkpoint using only ML-DSA keys; missing quorum → checkpoint rejected. Size: ~3.3 KB × validators per checkpoint | post-G2 |
| **PQ-5** | PQ light client for IBC: counterparty verifies PQ-4 checkpoints (raw, then a Jolt/Akita proof) | Counterparty accepts a membership proof anchored only in PQ-4 | research |

Until PQ-3 and PQ-4 ship, the accurate public claim is: **"JunoClaw verifies
post-quantum signatures on-chain in contracts."** Not "a post-quantum chain."

---

## 7. Lattice Jolt / Akita vs MAYO-5

These solve **different problems**, so "cheaper" depends on what you count.

- **MAYO-5 / ML-DSA** verify *one signature*. Measured: MAYO-5 verify tx =
  725 804 gas in-contract.
- **Akita** is a lattice polynomial commitment scheme (Module-SIS, transparent
  setup). **Lattice Jolt** is a16z's zkVM with Akita replacing the
  elliptic-curve Dory backend. It proves *arbitrary computation*: proofs are
  ~61–80 KB, verification is sublinear, and the prover got 1.3–2.2× faster
  than Jolt-with-Dory (per the Akita paper, ePrint 2026/1983, and
  LayerZero/a16z announcements).

**Per single signature: MAYO/ML-DSA wins by a wide margin.** A 60–80 KB proof
is already far larger than a signature before you pay for verification.
Verifying an Akita proof in Wasm has not been measured by us. It is
expected to be well above one MAYO-5 verify, and it would need a native
verifier (PQ-2 style) to be practical.

**Amortised over many signatures: Jolt/Akita wins.** Proving "≥2/3 of 100
validators ML-DSA-signed checkpoint X" (~330 KB of raw signatures) as one
<100 KB proof is exactly PQ-4 → PQ-5. That is where it belongs.

**Maturity:** released in 2026, not audited, no zero-knowledge mode
(`akita` and `zk` are mutually exclusive — irrelevant for us since
checkpoints are public). **Decision:** do not put it on the testnet critical
path. Track it for PQ-5 and prototype off-chain once PQ-4 exists.

---

## 8. Security & operations

- **S1** External review of `junoclaw-mayo-verify` against the reference implementation (G2).
- **S2** Validator key handling doc: BLS share, ed25519 identity, where they live, rotation (G1).
- **S3** Genesis ceremony rehearsal with external validators (`ceremony-test/` exists) (G1).
- **S4** Monitoring: finalized height, view timeouts, mempool depth, per-node `app_hash` agreement, relayer liveness. Alert on `app_hash` mismatch (G1).
- **S5** Faucet for `ujclaw` with rate limits (G2).
- **S6** Status page / thin explorer: height, peers, last finality, contracts, `GetTx` lookup (G2).
- **S7** Relayer under a supervisor (systemd/docker restart policy) + status endpoint (G1).
- **S8** Incident runbook: chain halt, `app_hash` divergence, relayer stuck, key compromise (G1).
- **S9** Contract backlog: `Relink` (address rotation), `BreakChannel` depth cap, ML-DSA pk hash alongside MAYO (G1).

## 9. Discipline for an AI-built chain

The chain is built largely by AI pair-programming. That will draw scrutiny,
and it should. Rules:

1. **Consensus-path changes** (`node.rs`, `app.rs::finalize_block`, `relay.rs`) require: a determinism argument in the PR, unit tests for every new rejection rule, and a 4-node devnet run before merge.
2. **No claim ships in an article without a tx hash, test name, or log line behind it.**
3. **Human review on every consensus and crypto diff**, even if it's one line.
4. Keep this plan and `docs/` current. Stale docs are how AI-built systems drift from reality.

## 10. Reference data from other chains

Used above; listed so the numbers can be challenged.

| Source | Data point |
|---|---|
| Cosmos SDK | secp256k1 sig verify = 1 000 gas; tx size = 10 gas/byte |
| CometBFT | Mempool gossips txs and rechecks them after every block; default `max_tx_bytes` 1 MiB; default block `max_bytes` 21 MiB; proposer-based timestamps (PBTS) in v1 |
| wasmd | Default `MaxWasmSize` 800 KiB |
| Ethereum | `ecrecover` precompile = 3 000 gas |
| Solana | No global mempool; txs forwarded to upcoming leaders (Gulf Stream) |
| NIST | ML-DSA = FIPS 204 (2024). MAYO = additional-signatures candidate, not standardized |
| Akita / Lattice Jolt | Proofs 61–80 KB, Module-SIS, transparent setup, no zk mode (ePrint 2026/1983; jolt.a16zcrypto.com) |

## 11. Sequencing

1. **Now → G0:** C1, C3, C4, C5, M1–M3, M5–M6, Q6–Q7, W2–W3, deploy `GetTx` to devnet.
2. **G0 → G1:** C2, C6, C7, C9 soak, M4, M7, Q2–Q4, PQ-1, S2–S4, S7–S9.
3. **G1 → G2:** Q5, Q8, PQ-2, S1, S5–S6, public docs.
4. **Post-G2:** PQ-3, PQ-4, then PQ-5 research with Jolt/Akita.

### Publication milestones

- **Article 1 (PQ first verify)** — publish once `GetTx` is live on devnet and a MAYO tx is fetched through it.
- **Article 2** — "Proposer can't lie": C1 + M1 + 4-node soak with a byzantine proposer. That's the milestone that makes the consensus story credible.
- **Article 3** — PQ-1 complete (fuzzed + all KATs incl. ML-DSA) → closed testnet (G1) announcement.
