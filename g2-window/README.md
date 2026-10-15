# G2 Join Window — Live Set Change Runbook

**Status: STAGED — not yet executed.** This window halts `junoclaw-g1`,
RESHARES the BLS threshold to the new set via dealer-free DKG (the group
public key — and the Osmosis 08-wasm IBC trust root — is preserved), and
restarts on a shared `restart_anchor`. All steps must be checked before
running.

R9 proved the mechanics on devnet (add 4→5, remove 5→4, retired-seat
isolation). The reshare path is verified: `dkg-synthesize-genesis-output`
reconstructs the coordinator-dealt G1 key as a canonical round-0 `Output`,
and deal/ack/log/finalize/observe completed against real G1 shares with
the group key unchanged.

**Why reshare, not re-deal:** a fresh `assemble-genesis` produces a NEW
group key → invalidates the IBC client trust root. Resharing keeps it.
Every later set change (4→5 for ffern, 5→6 for POSTHUMAN, …) reuses this
flow with `--previous` = the previous window's `dkg-output-<n>.json`.

## Set plan

| seat | host | G1 | G2 | action |
|---|---|---|---|---|
| ovh-uk1 | 51.195.201.74 | index varies | keep + deal | ssh `ubuntu@` + `ovh_g1` key |
| home-vb | WSL Ubuntu-24.04 | keep | keep + deal | `wsl -d Ubuntu-24.04 -u root` |
| vahana | 57.129.103.229 (Ravi) | keep | keep + deal | external — needs Ravi's action |
| akash | 196.216.137.249 | dead | **retire** | lease closed — excluded |
| anatoly | Hetzner DE (confirmed) | — | **join** | share-request needed first |
| ffern / POSTHUMAN | TBD | — | later windows | 4→5 / 5→6 reshares, same flow |

Window 1 runs at n=4: the three live seats (as dealers, resharing) plus
Anatoly (as new player). Dealers must be ≥ quorum (3) of the previous
set — the three live seats are exactly quorum, so all three must deal.

## Ordering constraint (important)

`restart_anchor` must be `>=` every seat's executed tip. A seat *ahead*
of the anchor diverges; a seat *behind* catches up via `ensure_executed`
— but only if a peer holding that state is already up. Therefore:

1. Read each seat's executed tip after halt.
2. Anchor = the **highest** tip observed.
3. Start the anchor-holding seat(s) **first**, then the rest.

## Steps

Phase A — coordinate (no downtime yet):
1. Collect `share-request.json` from every G2 participant into
   `g2-window/share-requests/<moniker>.json`. Existing seats reuse the
   files under `g1-ceremony/<seat>/share-request.json` (identity
   unchanged — only BLS shares change). New operators run
   `generate-testnet-keys keygen-share` locally and send only the
   share-request (public material; keys.json never leaves their host).
   Put the *dealing* seats' requests (surviving G1 members holding
   shares: ovh, home-vb, vahana) under `g2-window/dealers/`, and one
   finalized G1 `keys.json` under `g2-window/g1-keys/` (used only to
   synthesize + verify the round-0 output — it is public polynomial
   material plus a share verify, no new secrets are exposed).
2. `./deal.sh` — synthesizes `ceremony/dkg-output-0.json` (round-0
   Output of the G1 key, verified against every G1 keys.json in
   `g1-keys/`), then writes `ceremony/dkg-info.json` for round 1.
3. Dealers run `dkg-deal`, players run `dkg-ack`, dealers run
   `dkg-log`, coordinator runs `./observe.sh`, players run
   `dkg-finalize` — see the printed instructions and
   `docs/DKG-CEREMONY-RUNBOOK.md`. No `bls-share.json` ever travels;
   each operator derives their own share locally.
4. `./observe.sh` — writes `ceremony/shared.json` + `dkg-output-1.json`
   and ASSERTS the group key is unchanged.

Phase B — the window (chain halts):
5. `./halt.sh` — stops every seat service.
6. `./read-tips.sh` — queries each seat's executed tip; writes
   `g2-window/anchor.env` (`ANCHOR_HEIGHT`, `ANCHOR_DIGEST`).
7. `./apply.sh` — per seat: `dkg-finalize` (if not already done) +
   `finalize` (bind share to keys.json), write new `node.toml` via
   `mk-node-toml.py` (new peer table + `restart_anchor`), move
   `/data/consensus` journal aside (preserve, don't delete).
8. `./start.sh` — start anchor-seat first, then the rest.

Phase C — verify:
8. `./verify.sh` — confirms all seats finalizing, checks the new seat
   appears in leader rotation, confirms `peers` count = n−1, and that
   the retired seat cannot rejoin.

## Rollback

Journals are moved to `/data/consensus.g2-bak` (not deleted). To roll
back: restore old `keys.json` + `node.toml` + journal dir on every
seat, remove `restart_anchor`, restart. Rollback only works if *all*
seats roll back together — never mix old and new polynomial.
