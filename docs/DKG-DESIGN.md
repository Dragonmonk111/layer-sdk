# Dealer-Free DKG for the BLS Threshold Set

**Status: PLANNED — design only, nothing implemented.**

## Why

Today the BLS12-381 threshold polynomial is dealt by a coordinator
(`generate-testnet-keys assemble-genesis`). The coordinator transiently
holds every validator's private share — a single point of trust that is
acceptable for a testnet but not for mainnet. A cert requires the BLS
threshold *and* a MAYO2 quorum, so a malicious coordinator still cannot
forge finality alone — but it could collude with fewer-than-threshold
seats to reach quorum, which weakens the security argument we make to
operators. G2 can ship with coordinator dealing; mainnet cannot.

## Goal

Replace coordinator dealing with a dealer-free DKG where:

- No single party ever holds more than one share of the secret.
- Every participant can verify their share against a public commitment
  without trusting any other participant.
- The on-chain public polynomial is verifiable by anyone watching the
  ceremony (transcript is public; shares are private).
- The ceremony recovers cleanly from a participant who drops or
  misbehaves (identifiable aborts, not silent failure).

## Protocol sketch — Pedersen-style DKG over BLS12-381

n participants, threshold t (G2: n=5, t=4 — or whatever the set votes).

**Round 1 — commit.** Each participant i picks a random degree-(t−1)
polynomial f_i(x) and broadcasts Feldman commitments
C_i = [f_i(0)·G, a_i1·G, …, a_i(t−1)·G] plus a Schnorr proof of
knowledge of f_i(0) (the constant term). All commitments are posted
into a single ordered transcript.

**Round 2 — share.** Each i computes s_ij = f_i(j) for every j and
sends it to j over an authenticated, encrypted channel (the
participant's Ed25519 identity key is the channel keypair — the same
key that authenticates P2P today).

**Round 3 — verify + complain.** Each j checks
s_ij·G == sum_k C_i,k · j^k. Bad share → broadcast a signed complaint
with the offending share as evidence; the dealer is marked faulty and
excluded. Honest dealers' shares of a faulty dealer are re-dealt or the
dealer's contribution is dropped entirely.

**Round 4 — finalize.** The group public key is
PK = sum_i f_i(0)·G (sum of honest participants' constant-term
commitments). Validator j's private share is s_j = sum_i s_ij over
honest i. No party ever sees another's share — the polynomial only
exists distributed.

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

- Need a DKG round orchestrator (a coordinator still sequences the
  rounds — it just never sees shares). Could be a CLI the operators
  run, or a lightweight transcript service.
- Complaint adjudication: evidence is share + commitment — anyone can
  verify. On exclusion, decide between "re-deal n−1" vs "restart round".
- MAYO2 key stays per-operator local keygen — unchanged.
- Which library: `blst`/`ark-bls12-381` for the group ops — we already
  have `ark-bls12-381` in-tree for the BLS threshold.
