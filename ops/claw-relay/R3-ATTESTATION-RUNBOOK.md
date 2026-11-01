# R3 ceremony — per-seat UpdateAttestation runbook (Tuesday window)

Purpose: every seat binds its on-chain identity in `seat-attestation`
(juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm, code 21,
admin = dao-core juno122s3kcms43lgqu6p3h4tezcnwmpj5kslleku5jnz2a2jvat8mh0qkrqv3u).
Once `ListActiveSeats` returns all seats, the v5 relay's admission set is
fully on-chain — the member-bindings file is retired.

## What each operator needs BEFORE the window

1. **seat_index** — verify on-chain first, never assume from a table:
   `tx-sender query --contract <attest> --msg '{"get_seat":{"seat_index":N}}'`
   or `{"get_member":{"addr":"<your member addr>"}}`. If the seat is bound
   to a different member, `UpdateAttestation` fails with
   SeatBoundToOtherMember — the bound member (or dao-core) must
   `{"revoke_attestation":{"seat_index":N}}` first.

2. **hot_ed25519** (32B, base64 in JSON) — the node's P2P identity pubkey.
   DO NOT read keys.json on the node (rule 93). Source it from any PEER's
   node.toml peer list, or from the coordinator-side `pubkeys-only.json`
   roster exported at ceremony start.

3. **bls_share_pubkey** (96B compressed G2, base64) + **mayo_pubkey**
   (public) — same rule: export a `pubkeys-only.json` per seat at ceremony
   start, never open the live keys.json for fields.

4. **npub** (32B x-only secp256k1, base64) — YOUR relay identity. Generate
   fresh per seat (any nostr key tool; `openssl rand -hex 32` + a secp
   pubkey derivation, or `nostr-tools nsec keygen`). This is the key your
   daemon will AUTH with (NIP-42) when writing to the relay — keep the
   private half with the seat's ops files, never in the ceremony dir.

Contract checks that will reject your tx (contract.rs):
- sender must hold cw4 weight > 0 → NotMember
- seat bound to a different member → SeatBoundToOtherMember
- your member already bound to another seat → MemberAlreadyBound
- any key field duplicated across active records → key-not-unique errors
- empty hot/bls/npub → EmptyKey
Rotation of YOUR OWN seat is safe: old keys retire into KeyHistory so
evidence signed under superseded keys still resolves.

## The execute message

```json
{
  "update_attestation": {
    "seat_index": 5,
    "hot_ed25519": "<base64, 32 bytes>",
    "bls_share_pubkey": "<base64, 96 bytes>",
    "mayo_pubkey": "<base64 or omit field>",
    "npub": "<base64, 32 bytes>"
  }
}
```

## Path A — member is the deployer/ops account (coordinator runs it)

```bash
tx-sender execute --grpc <donor>:9090 \
  --contract juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm \
  --msg "$(cat attest-seat-N.json)"
```

(tx-sender derives the deterministic deployer key — the juno1dz87…
account that holds contract-deploy weight.)

## Path B — member is the operator's own wallet

Sign offline with `hybrid-sign` (member key never leaves the operator
host), then submit the raw tx via the coordinator:

```bash
tx-sender broadcast --grpc <donor>:9090 --tx-file signed-N.hex
```

## Ceremony order

1. Coordinator exports pubkeys-only.json per seat (pubkey material only —
   hot ed25519, bls share pubkey, mayo pubkey, seat index, peer IP). This
   file is SAFE to share — it is the sanctioned replacement for touching
   keys.json.
2. Each operator: verify seat_index via GetSeat/GetMember → confirm the
   binding is theirs or free → submit UpdateAttestation (path A or B).
3. After each tx: `{"get_seat":{"seat_index":N}}` — confirm member_addr,
   all four keys, attested_at timestamp.
4. When all 8 return: `{"list_active_seats":{}}` — expect 8 records,
   cw4-joined. THEN the relay admission refresh picks up the new set.
5. Smoke: relay logs show `admission source=attestation refreshed seats=8`
   on next refresh tick; a member npub writes through NIP-42 and is
   accepted; an unlisted npub is rejected.

## ⚠ Coverage gap — decide BEFORE the window (verified in contract source 2026-11-01)

`UpdateAttestation` binds `seat_index` to `info.sender` and requires
sender cw4 weight > 0 (`execute_update_attestation` → `NotMember`).
There is **no admin override path** — dao-core can only Revoke/Prune/
UpdateConfig, never attest on a member's behalf.

The cw4 group currently has **5 members** (operators with governance
weight). utsa, 79anvi, blockpro are **consensus-only — no cw4 weight by
design** (validator registration ≠ voting power). They cannot submit
`UpdateAttestation`, so `ListActiveSeats` caps at 5 records and the
runbook's "expect 8" is unreachable as written.

Options, in order of fidelity to the design:

1. **DAO proposal adding the three seats to cw4** (`UpdateMembers` on
   cw4-group) — grants them attestation rights BUT also governance
   weight: this is a policy decision, not an ops workaround. If ratified,
   it happens as a DAO vote before or inside the window.
2. **Attest the 5 member seats; consensus-only seats join relay via the
   local `--allowlist`** — works for the single-relay cutover; break-glass
   entries never federate, so vahana's relay would not admit them either
   until its operator lists them too.
3. **Contract upgrade adding a delegated-attestation path** (dao-core
   attests for a non-member seat, or cw4 gains a zero-weight
   consensus-role flag) — the principled fix; a contract change is its
   own RC, not a Tuesday add-on.

Default for the window is option 2 unless the DAO has already ratified
option 1 — flag this in the ceremony invite so no operator arrives
expecting a tx they can't sign.

## Rollback / failure

- Wrong key bound: UpdateAttestation again with the correct key (own-seat
  rotation is allowed and retires the bad key into KeyHistory).
- Seat bound to wrong member entirely: bound member submits
  RevokeAttestation (or dao-core via proposal) → correct member claims.
- A seat that skips the window: its npub simply isn't in ListActiveSeats
  — relay writes from that npub are denied until it attests. The chain
  does not care; only relay admission does.
