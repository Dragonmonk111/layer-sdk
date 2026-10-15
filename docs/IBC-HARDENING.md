# IBC Hardening — from devnet-grade to production-grade

**Status: PLANNED.** The ICS-20 G1 demo proved the outbound direction
end-to-end (stock ibc-go counterparty verifying our BLS threshold certs).
This doc is the gap list + design for the inbound direction and lifecycle.

## What is nominal today (disclosed, not hidden)

`packages/app/src/ibc/keeper.rs` stores commitments and enforces state
transitions, but the proof-bearing messages
(`ConnectionOpenAck`, `ChannelOpenAck`, `Acknowledgement`, `Timeout`,
`RecvPacket` on the counterparty-initiated side) carry proofs in their
`..` fields and do not verify them. A relayer can currently fabricate an
acknowledgement or a connection confirmation and the keeper will apply it.
Acceptable for a devnet demo; must be fixed before mainnet.

## Design: verifier boundary in the keeper

Introduce a `ClientVerifier` seam at the keeper level — every
proof-bearing entry point resolves the connection's client, loads its
client state, and calls:

```
verify_membership(client_state, height, proof, merkle_path, expected_value) -> bool
verify_client_update(client_state, signed_header, validator_set) -> new_state
```

Then plug in the real implementation:

- **Option A — in-keeper Tendermint client (recommended).** Port the
  07-tendermint verification rules: validator set + commit signatures
  (Ed25519 vote verification, `>2/3` voting power, `LastCommit` hash
  binding), plus ICS-23 `CommitmentProof` verification of the value
  under the signed header's `AppHash`. Self-contained, deterministic,
  auditable — ~1.5–2k lines. Osmosis-side structures are already
  modeled in `tools/bls-relayer/src/ibc.rs` for reference.
- **Option B — 08-wasm client upstream of the keeper.** Mirror what
  osmo does to us: a CosmWasm contract verifying Tendermint light
  blocks. More moving parts (wasm code lifecycle on JunoClaw's own
  VM); only worth it if we want client-pluggable counterparty types.

Option A is the path of least new machinery and the audit surface is
smaller. The keeper keeps its message/state-transition logic; only the
verification seam is new. `ClientType` in client state records which
verifier applies (`07-tendermint` now; our own `10-junoclaw-bls`
identifier later for chains mirroring us).

## Trust-root lifecycle (group-key rotation)

The osmo-side 08-wasm client stores the JunoClaw BLS **group public
key** as its trust root. What happens on set changes:

- **Resharing (DKG round N+1 over the same key):** the group key is
  preserved — verified in the 5-party round-2 simulation. The client
  needs *no* update; only new headers under the same trust root. This
  is now the preferred set-change mode for continuing validators.
- **Fresh deal (new group key) — e.g. G1→G2 restart with new members:**
  the client's trust root must be replaced. Path: `MsgUpdateClient`
  carrying a rotation attestation — the *old* group's threshold
  signature over `(new_group_key, activation_height)`, verifiable by
  the existing client state. Add `rotate_group_key` to the 08-wasm
  contract's execute surface. Until that lands, a fresh deal means
  client replacement (governance-store new client, re-handshake) —
  documented, acceptable for testnet, not for mainnet.
- **Validator set growth without resharing** (seat added, same key
  holders keep shares): same trust root if the deal is a reshare; the
  client only learns the new `threshold_public_key` implicitly —
  unchanged under reshare.

## Packet-safety checklist (implementation targets)

- [ ] `verify_membership` enforced on `conn_open_ack`, `chan_open_ack`,
      `chan_open_confirm`, `recv_packet`, `acknowledgement`, `timeout`
- [ ] ICS-23 proof spec verification (iavl + tendermint specs) — osmo
      commits under `ibc` root prefix
- [ ] Packet replay: receipts stored; `recv` on ORDERS channel enforces
      sequence ordering
- [ ] Timeout: verify absence proof at `packetReceiptPath` before refund
- [ ] Ack path: `acknowledgement` verifies proof of ack commitment under
      `acks/{port}/{channel}/{seq}` before clearing our commitment
- [ ] Client freshness: reject proofs at heights beyond client
      `latest_height`; reject stale `update_client` (non-monotonic or
      untrusted-period) 
- [ ] Relayer-side: timeout refund path, ack retry, fee preflight,
      supervision (systemd), health endpoint — mostly exist in
      `tools/bls-relayer`; harden + document

## e2e test matrix (local-osmosis)

- [ ] handshake happy path (done manually — automate)
- [ ] transfer + recv + ack (done manually — automate)
- [ ] timeout refund after `timeout_timestamp` elapses
- [ ] forged ack rejected (proof absent / wrong value)
- [ ] forged conn_ack rejected
- [ ] relayer restart mid-packet (idempotent resume)
- [ ] stale client (height < proof height) rejected
- [ ] reshare round: client verifies post-reshare cert without update
- [ ] fresh deal: rotation attestation accepted; old key rejected

## Ordering vs G2

None of this blocks G2 — the demo direction is the one external
operators care about first, and inbound stays devnet-grade with the
disclosure preserved in public material. Before mainnet: verifier +
rotation + the full test matrix are **gates**.
