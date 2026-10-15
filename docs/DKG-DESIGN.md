# Dealer-Free DKG for the BLS Threshold Set

**Status: IMPLEMENTED** — `generate-testnet-keys` now carries the full
ceremony (`dkg-info` / `dkg-deal` / `dkg-ack` / `dkg-log` /
`dkg-finalize` / `dkg-observe`) over the upstream Joint-Feldman module.
Verified end-to-end in a 5-party simulation: all players independently
derived the same group key, `dkg-observe` reproduced it from public
logs only, and a round-2 resharing preserved the group key while
issuing fresh shares (the property that keeps the 08-wasm IBC client's
trust root valid across set rotations). Remaining: runbook + ceremony
rehearsal with real operators.

## Why

Today the BLS12-381 threshold polynomial is dealt by a coordinator
(`generate-testnet-keys assemble-genesis`). The coordinator transiently
holds every validator's private share — a single point of trust that is
acceptable for a testnet but not for mainnet. A cert requires the BLS
threshold *and* a MAYO2 quorum, so a malicious coordinator still cannot
forge finality alone — but it could collude with fewer-than-threshold
seats to reach quorum, which weakens the security argument we make to
operators. G2 can ship with coordinator dealing; mainnet cannot.

## Key discovery: the primitive already exists

`commonware-cryptography 2026.3.0` ships a complete Joint-Feldman DKG at
`bls12381::dkg` (GJKR99 construction, Desmedt97 resharing) with the exact
semantics this doc originally sketched — and better, it's built for the
same `commonware-p2p` transport the chain already runs. Full API:

- `Info<V, P>` — round config: dealers, players, optional previous
  `Output` (resharing), round number.
- `Dealer` — `start` emits a `DealerPubMsg` (public commitment
  polynomial, degree `2f`) plus per-player `DealerPrivMsg`s; `finalize`
  produces a `SignedDealerLog` — the dealer's transcript of commitments
  and player acks, for external verification.
- `Player` — `dealer_message` validates each dealer's share against its
  public commitment and returns a signed `PlayerAck`; `finalize`
  produces the player's final `Share`.
- `observe` — an external observer reconstructs the `Output` (public
  polynomial + player list) purely from public material — meaning anyone
  watching the transcript can confirm the deal succeeded and what the
  group key is, without seeing a single private share.
- Complaint/resharing paths are built in — a dealer whose share fails a
  player's Feldman check gets a `reveal` demand and is disqualified on
  bad reveal; resharing re-deals an existing `Output` to a new player
  set with fresh randomness.

What this means concretely: **we do not implement a DKG, we orchestrate
one.** The remaining work is a ceremony tool (`jc-dkg` or subcommands on
`generate-testnet-keys`) that wires these state machines to our
file/ops-channel transport — dealers emit `DealerPubMsg` + encrypted
`DealerPrivMsg`s, players ack, `observe` produces the `shared.json`
equivalent. No new crypto, no new wire format of our own design.

## Goal

Replace coordinator dealing with a dealer-free DKG where:

- No single party ever holds more than one share of the secret.
- Every participant can verify their share against a public commitment
  without trusting any other participant.
- The on-chain public polynomial is verifiable by anyone watching the
  ceremony (transcript is public; shares are private).
- The ceremony recovers cleanly from a participant who drops or
  misbehaves (identifiable aborts, not silent failure).

## Protocol shape (upstream Joint-Feldman, GJKR99)

n participants, threshold t. Every participant is both a dealer and a
player — dealers jointly create the key, players each hold a share.

- **Commit round**: each dealer emits `DealerPubMsg` — a public
  commitment polynomial (degree `2f`) — broadcast to all players.
- **Deal round**: each dealer sends each player a `DealerPrivMsg`
  (scalar evaluation at that player's index) over the authenticated
  channel — the operator's Ed25519 identity keys it.
- **Ack/complaint round**: each player verifies each share against the
  dealer's public commitment and returns a signed `PlayerAck`, or
  issues a `reveal` demand for a bad share — the dealer either reveals
  a consistent share or is disqualified.
- **Finalize**: players hold their final `Share`; dealers emit
  `SignedDealerLog`s; `observe` reconstructs the public `Output` from
  public material only — the group key is externally verifiable without
  ever exposing a share.

The polynomial never exists in one place. A single honest participant
suffices for the group key to be unrecoverable by anyone else.

## What changes vs today

| today (coordinator-dealt) | DKG |
|---|---|
| coordinator generates f, keeps all s_i | f never exists in one place |
| trust: coordinator honest + deletes shares | trust: ≥1 honest participant |
| `assemble-genesis` single-shot | 4-round interactive protocol |
| share-request.json (public only) | same + commitments/transcript |
| bls-share.json pushed to operator | share computed locally — nothing secret crosses the wire |

## Ceremony logistics (inherits today's ops shape)

- **Auth**: each participant's Ed25519 identity signs every round
  message — same key that peers already use for the authenticated
  p2p handshake. No new identity bootstrap.
- **Transcript**: all commitments, complaints, and the final validator
  set land in one signed, append-only document — the public artifact
  that proves the deal happened. Stored alongside `shared.json`.
- **Rounds over the ops channel** (Telegram/relay) — the ceremony is
  asynchronous-friendly: each participant posts when ready, the next
  round opens when all posts are in.
- **Failure handling**: a participant who misses a round is excluded
  after a timeout and the set deals n−1; a *complained* participant is
  excluded and the round re-runs without them.

## Migration path

G1/G2 sets keep coordinator-dealt shares. First DKG runs as a *set
change* on a live chain — same `restart_anchor` window as G2, but
`assemble-genesis` is replaced by the DKG transcript:

1. Window opens: participants run rounds 1–3 off-chain (no halt needed
   yet — the deal produces only keys).
2. Halt, anchor, write the DKG-produced `shared.json` + local shares
   into each seat's config, wipe journals, restart — identical to the
   coordinator-dealt window from this point on.

So DKG changes *how shares are produced*, not *how the chain adopts
them* — the set-change machinery (R9/G2 window) is already built.

## Dependencies / open questions

- **Crypto: none.** The DKG primitive is upstream
  (`commonware-cryptography::bls12381::dkg`) — same crate family,
  already-vendored version 2026.3.0. The remaining work is the ops tool.
- **Transport**: the protocol needs an authenticated dealer→player
  channel for `DealerPrivMsg`. Two paths: reuse the operators' Ed25519
  keys for an encrypted file exchange over the existing ops channel
  (simplest — matches how `bls-share.json` moves today), or run a
  short-lived `commonware-p2p` ceremony network (heavier; only worth it
  if ceremonies become frequent).
- **Round orchestration**: a coordinator still sequences the rounds —
  it just never sees shares. A CLI the operators run is enough; the
  `observe` function makes the outcome publicly verifiable regardless.
- Complaint adjudication is handled by the protocol (reveal → disqualify);
  the ops question is only "re-deal n−1 or restart round" per failure.
- MAYO2 key stays per-operator local keygen — unchanged.
