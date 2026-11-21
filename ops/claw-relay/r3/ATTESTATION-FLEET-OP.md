# Seat-attestation fleet op — corrected signing model (2026-11-21)

Verified on-chain this date via `tx-sender query` against
`127.0.0.1:9090` (OVH donor gRPC):

- `list_active_seats` on `juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm` → **empty** — no seat is attested.
- `list_members` on cw4 `juno1u0l6ddahjn88v4usdrk33vk4k53qsjmmun45n9ssvfl5sg5xnj3qul37pj` → exactly 5 members, weight 1 (mapping per `docs/AGENTS-DAO-PLAN.md`):

  | member addr | seat | key location | funded |
  |---|---|---|---|
  | `juno134zsp2umr4vkhkvrn35uy3fv4n5uss233vnraz` | ovh-uk1 (seat 5) | `/opt/dao-keys/dao-key.pem` on OVH | 42.59 JCLAW ✓ |
  | `juno1wjtmj0fekn4fkdpf8ttgctr537txvarae225a0` | home-vb (seat 3) | `/opt/g1-node/keys/dao-key.pem` on retired WSL env — needs moving to the VB guest | ~1.0 JCLAW ✓ |
  | `juno1yzte2lj9mtn2jlquj28apg82aedrxwywjt0k0m` | posthuman (seat 2) | operator | ? |
  | `juno1p3suqqsdzz0wd30asz3uzd0u3pfwsg36g6el8e` | anatoly (seat 0) | operator | ? |
  | `juno1xqyqszj0nppyhjlnywm8jwd7ye9hrphnz0vawk` | vahana/Ravi (seat 6) | operator | ? |

  Note: `juno134zsp2…` doubles as the transcript-anchor wallet; its
  BUZZ-PARITY InsufficientFunds flag is stale — topped up since.

## Correction to R3-ATTESTATION-RUNBOOK "Path A"

`tx-sender`'s deterministic deployer key is `juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992`
— **not a cw4 member**. The coordinator cannot submit `UpdateAttestation`
on anyone's behalf: the contract binds `seat_index` to `info.sender` and
requires sender weight > 0 (`NotMember`). There is no admin path —
every member signs their own tx.

## Per-member procedure (the only path)

1. Find your seat + addr:
   `tx-sender query --grpc <donor>:9090 --contract juno1rhlaj…t7qm --msg '{"get_member":{"addr":"<your juno1…>"}}'`
   If your addr already holds a seat, the record shows the current binding —
   re-submitting rotates keys into KeyHistory (safe).
2. Take your seat's `update_attestation` object from
   `ops/claw-relay/r3/seat-roster.json` (coordinator pre-staged: seat_index,
   hot_ed25519, bls_share_pubkey, mayo_pubkey — all public material).
3. Generate your relay npub — 32-byte x-only secp256k1 pubkey. ANY nostr
   key tool (`nostkeygen`, `nak key generate`, `buzz-cli users set-profile`
   keygen). Keep the nsec with your seat ops files — it's what your daemon
   NIP-42-AUTHs with. Put the pubkey (base64) into the `"npub"` field.
4. Sign `MsgExecuteContract` offline with `hybrid-sign` on your member
   key (key never leaves your host), or broadcast from your own wallet.
5. Broadcast: `tx-sender broadcast --grpc <donor>:9090 --tx-file signed.hex`
   (any seat's gRPC donor works; ours: api.junoclaw.xyz → :9090).
6. Gas: a few hundred ujclaw. If your member addr is unfunded, say so in
   group — the deployer faucet covers it.

## Coordinator checklist

- [ ] Fill remaining npub fields in seat-roster.json as members publish theirs (or members fill locally — roster is theirs to complete).
- [x] Top up `juno134zsp2…nraz` — already holds 42.59 JCLAW; InsufficientFunds flag was stale. (Faucet had a stale-sequence bug — provided 0 vs expected 196; fixed by restart 2026-11-21.)
- [x] Our second member identified: `juno1wjtmj0…` = home-vb, key at `/opt/g1-node/keys/dao-key.pem` on the retired WSL env — move to VB guest `/mnt/extdrive/` at op time. No new addr needed — DO NOT create one (a third member would need a DAO UpdateMembers vote anyway).
- [ ] After each member tx: `{"get_seat":{"seat_index":N}}` → member_addr + 4 keys + attested_at.
- [ ] When 5 records land: `{"list_active_seats":{}}` → 5 seats → relay admission refresh picks them up (`admission source=attestation refreshed seats=5` in claw-relay logs).
- [ ] Consensus-only seats (utsa, 79anvi, blockpro, LuckyStar, ALXY, ANODE) → `--allowlist` npubs, per TUESDAY-WINDOW.
- [ ] Then the relay flip: drop `--member-bindings`, add `--attestation-contract`, restart, smoke 403 on unlisted npub.

## Delegated-attestation gap (unchanged)

Consensus-only seats still can't attest — options from the runbook
(DAO UpdateMembers / allowlist / contract upgrade) stand. Default
remains option 2 (allowlist) until the DAO decides otherwise.

## Group message (paste-ready)

> Attestation op — the missing piece before the relay reads admission from chain.
> Checked on-chain: `list_active_seats` is empty; the cw4 group has exactly 5 members.
> **Correction to the old runbook: I cannot submit for you** — the contract binds the
> seat to the tx sender, and the coordinator deployer addr isn't a member. Each of the
> 5 members signs their own `UpdateAttestation`.
>
> If you're a member (anatoly, POSTHUMAN, me×2, Ravi):
> 1. your seat's `update_attestation` msg is pre-staged in `seat-roster.json` (I'll send
>    your block in DM — public material only)
> 2. generate a fresh nostr keypair — the pubkey goes in `"npub"`, the nsec stays with you
> 3. sign `MsgExecuteContract` with `hybrid-sign` on your member key, broadcast to any
>    donor gRPC
> 4. need gas? say it — deployer covers
>
> Consensus-only seats (utsa, 79anvi, blockpro, LuckyStar, ALXY, ANODE): no tx — your
> relay npubs go into the `--allowlist` instead. Send me your npub when you have one.
>
> Target: 5 records in `list_active_seats`, then the relay drops the static
> member-bindings file and reads admission live from chain.
