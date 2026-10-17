# DKG Ceremony Runbook — operator instructions

**Dealer-free threshold key ceremony.** Unlike G1 (where a coordinator
dealt shares), no party — including the coordinator — ever holds the
group secret. Every participant deals AND holds a share.

Verified end-to-end in a 5-party simulation including a resharing
round. Requires `generate-testnet-keys` built from `commonware` branch
(rc11+).

## What you need

- `generate-testnet-keys` binary (cargo build in `tools/generate-testnet-keys`)
- Your existing `keys.json` (from `keygen-share`) — Ed25519 + MAYO identity
- `dkg-info.json` from the coordinator

Since v0.6.0-rc12 the `dealing-priv-*` files are sealed to each
recipient's Ed25519 identity (ephemeral X25519 + XChaCha20-Poly1305) —
they can travel over any channel, including a coordinator's public
mailbox, because only the addressed `keys.json` can open them. Treat
them as public-ish artifacts; `dealer.seed` remains the sensitive file.
(Files dealt with `--plaintext-priv`, or by pre-rc12 binaries, still
work — the reader accepts both formats — but contain cleartext shares
and must use the old private-channel delivery.)

## Step 0 — coordinator publishes the round spec

```
generate-testnet-keys dkg-info \
  --requests share-requests/ --chain-id junoclaw-g2 --round 1 \
  --out dkg-info.json
```

The file is public and deterministic — verify it lists your
`ed25519_public_hex` in `players` (and `dealers`).

## Step 1 — deal (each operator)

```
generate-testnet-keys dkg-deal --info dkg-info.json --keys keys.json \
  --out-dir ./dkg-out
```

Outputs:

- `dealer.seed` — 32-byte secret. **Keep private; needed for `dkg-log`.**
- `dealing-pub-<you>.json` — broadcast to all operators.
- `dealing-priv-<you>-for-<peer>.json` — one per player, sealed to that
  recipient's identity key. Safe to send via a shared coordinator
  mailbox; a relaying coordinator cannot read them. (rc12+)

Re-running is safe and reproduces identical files (seeded).

## Step 2 — ack (each operator)

Collect every `dealing-priv-*-for-<you>.json` into a dir, then:

```
generate-testnet-keys dkg-ack --info dkg-info.json --keys keys.json \
  --priv-dir ./for-me --out-dir ./acks
```

Per dealer: `acked dealer <id>` = your share verified against their
public commitment → ack file written. `COMPLAINT` = bad share → do not
ack; that dealer must publish your share in their log (you'll still
recover it — the protocol reveals it for you).

Send each `ack-<you>-to-<dealer>.json` to its dealer (acks are signed,
public-safe).

## Step 3 — publish log (each dealer)

```
generate-testnet-keys dkg-log --info dkg-info.json --keys keys.json \
  --seed-file ./dkg-out/dealer.seed --acks-dir ./acks-for-me \
  --out-dir ./logs
```

Collect the `ack-*-to-<you>.json` files, run this, broadcast
`log-<you>.json` to everyone. **Delete `dealer.seed` after.**

## Step 4 — finalize (each operator)

```
generate-testnet-keys dkg-finalize --info dkg-info.json --keys keys.json \
  --priv-dir ./for-me --logs-dir ./logs --requests ./share-requests \
  --out bls-share.json

generate-testnet-keys finalize --keys keys.json --bls-share bls-share.json
```

Produces the same `bls-share.json` shape the old coordinator dealt —
your node config is unchanged. The tool cross-checks your share against
the group polynomial before writing.

## Step 5 — anyone: public verification

```
generate-testnet-keys dkg-observe --info dkg-info.json \
  --logs-dir ./logs --requests ./share-requests --out-dir ./ceremony-out
```

Derives `shared.json` (group key, validator table) from the signed
dealer logs alone — no private material. Every operator should run this
and confirm the group key matches what `dkg-finalize` reported. Also
writes `dkg-output-<round>.json` — keep it; it's the `--previous` input
for the next resharing round.

## Failure handling

- **Player offline during ack:** their share gets revealed in each
  dealer's log — they can still finalize later. Too many reveals fails
  the round (start a new round number).
- **Bad dealer:** players complain, dealer either reveals consistent
  shares in its log or is excluded from the group key.
- **Dispute:** every message is signed and the full transcript is
  externally verifiable — post the logs to the coordinator.
- **Never reuse a round number.** Failed round → increment `--round`.

## Resharing (validator rotation, same group key)

For a set change that keeps the group key (preferred — doesn't touch
the IBC client's trust root):

```
generate-testnet-keys dkg-info --requests new-share-requests/ \
  --chain-id junoclaw-g2 --round <N+1> \
  --dealers continuing-share-requests/ \
  --previous ceremony-out/dkg-output-<N>.json --out dkg-info.json
```

Dealers = continuing validators holding round-N shares (must be ≥ the
old quorum); players = the new set. Continuing operators' keys.json
already carries `share_hex` — the tool picks it up automatically.
