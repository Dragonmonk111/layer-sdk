# DAO DAO on JunoClaw — Feasibility & Execution Plan

*Draft for discussion — shared with the DAO DAO team for review/edits.*

## TL;DR

JunoClaw is a sovereign Rust L1 (not Cosmos SDK, not Tendermint/CometBFT)
that already runs a **vendored CosmWasm 2.3.2 VM as a first-class module**
— store-code, instantiate, instantiate2, execute, migrate, submsg/reply,
admin ACLs, gas metering — and just completed a **live ICS-20 transfer
into a stock ibc-go chain** (Osmosis), verified on-chain by a BLS
threshold light client inside an 08-wasm contract — and has now run a
**live five-operator validator-set change** (G1 → G2): three
independent operators joined/coordinated a dealer-free Joint-Feldman
reshare that preserved the BLS group key, then the network halted and
resumed finality under the new 4-of-5 set.

We'd like to port the DAO DAO contract suite onto it as the chain's
native governance surface. As far as we can tell, DAO DAO core is pure
CosmWasm with no SDK dependencies below the VM — which makes this a
porting exercise, not a redesign, and (to our knowledge) the first DAO
DAO deployment on a non-Cosmos-SDK chain. That's exactly the
chain-agnostic-wasm thesis: same contracts, different sovereign
substrate.

## The chain, briefly

- **Consensus:** Commonware Simplex — single-slot BFT, ~0.5 s blocks.
  Every finalized block carries a BLS12-381 threshold certificate
  (recoverable quorum signature), plus a MAYO2 post-quantum certificate
  bitmap-addressed to the same signer set. Finality is a certificate,
  not a heuristic.
- **Wasm:** `packages/app/src/wasm/` — a full x/wasm-style module over
  vendored `cosmwasm-vm 2.3.2` (patched with `bn254_*` host functions).
  `hackatom` and a `bank_cw20` test contract already execute in-tree.
- **IBC:** the chain writes ibc-go-encoded commitments at ICS-24 paths
  in its Merkle state; the counterparty verifies them via an 08-wasm
  BLS light client. Voucher mint + ack verified live on Osmosis.
- **Indexer:** CDC archiver projects `_txres` results and address
  backlinks into SQLite — so contract events and DAO activity are
  queryable off-chain without new infra.

## Why DAO DAO specifically

DAO DAO is the most complete governance primitive in the ecosystem:
`dao-core` + proposal modules + voting modules + `cw-admin-factory`
SubDAOs + token/NFT/membership voting — all as auditable contracts with
no privileged chain module. For a young sovereign chain, that means real
DAO governance on day one instead of building governance from scratch.

Our immediate use is validator operations — which is also the honest
path toward in-protocol validator rotation:

```
Juno Agents DAO (dao-core)
├── Validator Ops SubDAO   — set-change ratification, window
│                             scheduling, operator admission/removal
├── Treasury SubDAO        — community spend, audit funding
├── Infra SubDAO           — public endpoint funding, grants
└── (later) Params SubDAO  — on-chain config once native gov exists
```

Sequence: today the SubDAO votes and humans execute the set-change
window; the next step is a passed proposal becoming the on-chain signal
for a set-change at the next epoch boundary. The DAO isn't decoration —
it's the governance layer the chain's validator lifecycle is meant to
converge on.

## What we think the port needs (audit list)

DAO DAO doesn't use the SDK gov module, so the dependency surface is
narrow. The risks we'd verify first, in order:

1. **Nested submsg/reply depth** — DAO instantiation spawns proposal
   modules which spawn voting modules, ~3 levels of `SubMsg` + `reply`
   in one tx. Our keeper implements reply handling, but this is the
   most common failure mode for wasm ports. Test: instantiate
   `cw-admin-factory` → `dao-core` → `dao-proposal-single` +
   `cw20-stake` on devnet.
2. **Contract-to-contract queries** — voting modules query staking
   state at proposal/execution time; proposal modules use
   `block.height`/`block.time` snapshots. Confirm our `query` path and
   `Env.block` fields behave the way the contracts expect.
3. **Address format** — contracts are bech32-agnostic, but any
   chain-registry/bech32 assumptions in off-chain tooling need
   remapping to our prefix.
4. **Events → indexability** — DAO UIs live off contract events; ours
   now flow into `txs`/`tx_addrs` projections. Confirm the attributes
   DAO DAO emits are captured (they should be — we index event
   attributes generically).
5. **Anything we haven't found** — this is where we'd most value DAO
   DAO team eyes: which contracts or ops have you seen break on
   non-standard chains? (e.g. `dao-migrator`, `cw-tokenfactory` voting,
   polytone/IBC-flavored modules we'd skip initially.)

## Execution phases

- **P0 — audit (days):** devnet deploy of the `dao-contracts` suite;
  instantiate a minimal DAO; produce a concrete gap list.
- **P1 — live DAO (days):** fix porting gaps; deploy the suite on the
  public testnet; instantiate Juno Agents DAO + Validator Ops SubDAO
  with the testnet operators as initial members.
- **P2 — ops console (weeks):** a focused validator-ops frontend —
  proposals, votes, treasury, operator registry — over the wasm query
  path + the archiver index. We are *not* planning to fork
  `dao-interface` initially (its chain-registry coupling is heavy); a
  purpose-built console is faster and we can revisit.
- **P3 — dogfood:** run the next validator-set change through a
  SubDAO vote, then hand execution to the operators. (The G2 change
  already ran the full lifecycle manually — DKG reshare, halt, anchor,
  coordinated restart — so the operational surface a DAO would govern
  is proven, not theoretical.)
- **P4 — coordinate upstream:** if the port works, the interesting
  outcome is DAO DAO officially shipping on a non-Cosmos chain. We'd
  rather upstream any contract changes than fork the suite — and we
  suspect a "DAO DAO runs on a sovereign Rust chain" announcement is
  valuable to you too.

## What we'd want from the DAO DAO side

1. Sanity-check the audit list — anything contract-side you know is
   tightly coupled to SDK assumptions?
2. Guidance on which suite version/artifacts to pin for the port.
3. Willingness to treat a JunoClaw deployment as an official (or at
   least blessed) target if the audit passes — including, eventually,
   a chain-registry entry / interface support conversation.
4. A contact point for the inevitable edge cases during P0.

## Honest caveats

- JunoClaw is a young testnet network — G1 ran multi-operator for
  weeks including one public incident (root-caused, patched, deployed),
  and G2 now runs a five-operator set that onboarded via resharing and
  a coordinated halt/cutover. This is not a mainnet TVL pitch; it's a
  "your contracts are portable and here's a chain that proves it"
  pitch.
- Our wasm module is real but young — the P0 audit exists precisely
  because we expect edge cases. We'd rather find them on devnet than
  claim compatibility we haven't demonstrated.
- A fresh validator-set deal rotates the BLS group key; we use
  Joint-Feldman resharing to preserve it across set changes — just
  exercised live in the G1 → G2 transition, which kept the group key
  (and therefore the IBC client trust root) byte-identical across the
  operator change. DAO contracts themselves are unaffected either way.
