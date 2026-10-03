# JunoClaw — the chain

**The JunoClaw L1 consensus core: a lean Rust chain on Commonware primitives, running Simplex-family BFT with quorum-level post-quantum certificates.**

Project home → [github.com/Dragonmonk111/junoclaw](https://github.com/Dragonmonk111/junoclaw) · [junoclaw.xyz](https://junoclaw.xyz) · [Telegram](https://t.me/junoclaw) · [@junoclawdao](https://twitter.com/junoclawdao)

> If you arrived here from upstream Lay3rLabs — this fork is no longer Slay3r. It is the JunoClaw L1.

---

## What this repo is

This fork carries the **consensus and node code** for JunoClaw — the layer that produces blocks and certificates. The `commonware` branch is the live development line.

- **Hybrid consensus, running today** — `hybrid_consensus`: every vote carries a threshold-BLS partial *and* a 186-byte MAYO2 signature; every certificate requires quorum on both halves. `max(classical, PQ)` — not a migration roadmap.
- **State-certifying finality** — every block final binds the executed `state_root` into its certificate (~1 s). No confirmations, no reorgs.
- **Chain-linked certificates** — each cert binds the previous one; light clients verify certs, not header chains.
- **Deterministic state machine** — KV + Wasm execution, certified `state_root` per block, fail-stop divergence detection, certified state-sync snapshots, height-range backfill, durable payload store, tx indexing, gRPC + Simulate.

## Status

- **24-h chaos soak, ~175k blocks, zero divergence** — kill/restart, partitions, container recreates, Byzantine-proposer fault injection (`fault_inject`: `bad_state_root` | `bad_parent`). Two real bugs found *by* the soak, fixed in code (incl. the backfill fetch flood — C9).
- **Next: G1 public testnet** — external validators, key ceremony, faucet.

## Where the rest lives

- **Agent layer, contracts, circuits, WAVS/sealed-signer, docs, website** → [Dragonmonk111/junoclaw](https://github.com/Dragonmonk111/junoclaw)
- **Validator runbook** → `docs/` in this repo (liveness, lag tiers: backfill vs state-sync, divergence semantics, soak ops rules)

## Heritage

Forked from [Lay3rLabs/layer-sdk](https://github.com/Lay3rLabs/layer-sdk) ("Slay3r" — a pure-Rust CosmWasm chain). JunoClaw replaces the Tendermint/CometBFT core with Commonware-based Simplex-family consensus and adds the hybrid PQ certificate layer; the upstream substrate is retained where it earns its keep.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Upstream copyright and attribution in `NOTICE`. Code under `lib/cosmwasm` is vendored CosmWasm and remains Apache-2.0 only.
