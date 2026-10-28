# Hot↔Cold Attestation — seat key binding spec (draft v0.2)

Status: design draft, reviewed against the live DAO + relay code.
Prereq for: distinct npub/seat, relay admission, equivocation
attribution, agent-seat delegated votes.

## Problem

Two facts are currently unbound on-chain:

1. **Governance identity is cold.** A validator's authority is cw4
   membership — a DAO address signing proposals and votes. It should
   stay offline and must never sit on a validator host.
2. **Operational identity is hot.** The seat's keys live on the node:
   the ed25519 P2P identity, the BLS12-381 share that signs vote
   partials, the MAYO2 key — plus a third hot identity, the relay
   npub (currently *shared* across seats: `d121b7da`, which destroys
   per-seat attribution).

Nothing on-chain says "seat idx3's keys belong to cw4 member X."
Without that binding: relay allowlists can't be derived from
membership, equivocation evidence can't be attributed to a governance
member, and an agent seat can't prove its actions belong to the
member that ratified it.

## Design — a `seat-attestation` CosmWasm contract

A small contract (not a native module — auditable, DAO-upgradeable,
queryable over `/api/wasm/smart` and gRPC; contract-to-contract
queries are live-proven in this VM port — `voting_power_at_height`
already crosses voting→staking→token on chain).

```rust
InstantiateMsg {
    cw4_group_addr: Addr,   // authority: who may attest
    dao_core_addr:  Addr,   // admin: revokes, upgrades, rebinds
}

SeatRecord {
    seat_index:       u8,     // consensus validator index (0..n)
    member_addr:      Addr,   // cw4 member (cold, governance)
    hot_ed25519:      Binary, // P2P/peer identity key on the node
    bls_share_pubkey: Binary, // signs consensus vote partials —
                              // the key equivocation verifies against
    mayo_pubkey:      Option<Binary>, // PQ partials, same role
    npub:             Binary, // 32-byte x-only secp256k1, lowercase
                              // hex (BIP-340) — the relay identity
    attested_at:      u64,
}
KeyHistory = Map<(seat_index, seq), {hot, npub, attested_at, superseded_at}>
```

**Write rules**

- `UpdateAttestation { seat_index, keys… }` — sender must hold cw4
  weight (contract smart-queries the group inside execute). Fails if
  `seat_index` is bound to a *different* member: rebinding requires
  `RevokeAttestation` by the bound member or a dao-core execute —
  otherwise last-writer-wins is seat-stealing.
- `RotateHotKey { seat_index, new_hot, new_npub }` — deliberately
  cold-signed each rotation in v0.2 (a pre-signed rotation-cert
  variant is noted as an option; the cold ceremony per rotation is
  the defensible default at this fleet size).
- `RevokeAttestation { seat_index }` — the bound member, or a sender
  equal to stored `dao_core_addr` (a passed DAO proposal executes as
  dao-core — an ordinary sender check, not a privileged API).
- `PruneSeat { seat_index }` — permissionless: revokes any record
  whose `member_addr` no longer holds cw4 weight. Closes the
  stale-record gap when a member leaves.
- `SubmitEvidence { seat_index, view, partial_a, partial_b }` —
  permissionless; the contract verifies both conflicting BLS
  partials against the bound `bls_share_pubkey`, dedups on
  (seat, view), and stores the pair for a DAO proposal to reference.
  **The signer bitmap is not evidence** — it records presence, not
  which partials contributed; evidence must carry the actual
  conflicting partials (and finalize partials skip `verify()`
  upstream, so presence says even less).
- `UpdateConfig { cw4_group_addr?, dao_core_addr? }` — dao_core
  only. Mandatory: the DAO stack is redeployed at the bech32
  `juno→jclaw` cutover (and has already been re-instantiated once —
  a config bound to a dead group is a dead registry).

**Queries**

- `GetSeat { seat_index }`, `GetMember { addr }`, `ListSeats {}`
- `ListActiveSeats {}` — joins group membership server-side
  (records whose member currently holds weight) — **this is the
  allowlist source; `ListSeats` alone admits stale members.**

## Cardinality (stated explicitly — it was ambiguous in v0.1)

Membership seats ≠ signing seats. The DAO has 5 members; the fleet
has 8 (10 after R3). Model: **a seat is bound only if a cw4 member
claims it — one member may bind multiple seats** (operator seats),
and unbound seats simply aren't attestable (they're not members).
Every key field is globally unique across records — a member cannot
bind another seat's npub or hot key.

`seat_index` itself is **not on-chain verifiable** — roster order
lives in keys.json/toml, not in contract state. The honest model is
first-claim by a weight-holding member + DAO dispute via
`RevokeAttestation`. Say it plainly; do not pretend the contract
verifies reshare output.

## What consumes it

1. **claw-relay admission** — replaces the static file allowlist and
   retires `d121b7da`. Semantics the code needs (the current relay
   fails OPEN on an empty set — `!allowlist.is_empty()` admits
   everyone): a background task refreshes the allowlist from
   `ListActiveSeats` every N seconds (matching `cw4-relay-watch`'s
   5-min pattern) or per epoch; on query error it keeps the last
   good set; an empty result at first load denies writes. Never
   per-message contract queries.
2. **Buzz/relay messages** — seat-signed messages verify against the
   record's npub — attribution to a governance member, not an IP.
3. **Equivocation (RC18)** — `SubmitEvidence` resolves
   seat→member; a DAO proposal executes the penalty (weight
   reduction/removal — no stake exists to slash).
4. **Agent seats (P5)** — the delegated vote scope records against
   the same record: "hot key K may vote on proposal classes C under
   bounds B" — receipts tie agent actions to the binding.
5. **jc-status / explorer** — seat names become a chain query.

## Invariants

- The cold key never appears on a validator host — attestation txs
  sign from wherever the member keeps their DAO key.
- The hot keys never gain governance weight — weight lives only in
  cw4; the contract reads it, never mints it.
- History is append-only — rotated keys move to `KeyHistory` with
  validity windows so evidence signed under an old key stays
  attributable.
- The contract cannot assign seats — it binds claims; the DAO
  arbitrates disputes.

## Integration sequence

1. Contract (~250 lines, cw-plus style; no new crypto — cw4 query +
   storage + signature checks against `bls_share_pubkey`).
2. **Instantiate via a DAO proposal** (`wasm.instantiate` with
   `admin = dao_core` — the proven SubDAO path). `jc-instantiate`
   cannot set admin today (`admin: String::new()` hardcoded in
   `tools/bls-relayer`) — land a `--admin` flag or use the proposal
   route.
3. Seats file `UpdateAttestation` (cold-key txs) — part of the R3
   post-ceremony checklist.
4. claw-relay admission switches to the `ListActiveSeats` refresh
   loop; `d121b7da` retires.
5. P5 agent seat: first delegated-scope attestation.

## Deferred / open

- Pre-signed `RotateHotKey` rotation certs — cheap rotation without
  a cold ceremony. Deliberately deferred; revisit when seat count
  grows.
- Evidence partials for MAYO2 — `mayo_pubkey` is bound now so the
  evidence schema can extend to PQ equivocation later.
- SubDAO-held seats — works already: the SubDAO is itself a cw4
  member; its proposal flow signs the attestation.
