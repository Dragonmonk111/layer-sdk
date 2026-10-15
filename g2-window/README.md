# G2 Join Window — Live Set Change Runbook

**Status: STAGED — not yet executed.** This window halts `junoclaw-g1`,
re-deals the BLS threshold for the new set, and restarts on a shared
`restart_anchor`. All steps must be checked before running.

R9 proved the mechanics on devnet (add 4→5, remove 5→4, retired-seat
isolation). This is the same ceremony on the live chain.

## Set plan

| seat | host | G1 | G2 | action |
|---|---|---|---|---|
| ovh-uk1 | 51.195.201.74 | index varies | keep | ssh `ubuntu@` + `ovh_g1` key |
| home-vb | WSL Ubuntu-24.04 | keep | keep | `wsl -d Ubuntu-24.04 -u root` |
| vahana | 57.129.103.229 (Ravi) | keep | keep | external — needs Ravi's action |
| akash | 196.216.137.249 | dead | **retire** | lease closed — excluded |
| seat-5 | TBD operator | — | **join** | share-request needed first |

Until a 5th operator is ready, the window can run at n=4 over the three
live seats plus a local rehearsal seat on this host (fresh identity —
NOT the retired Akash key material, which stays retired).

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
2. `./deal.sh` — runs `assemble-genesis` over the share-requests,
   producing `g2-window/ceremony/shared.json` + per-seat
   `validator-<i>/bls-share.json` and `node-<i>.toml`.
3. Distribute each seat's `bls-share.json` over the encrypted channel.

Phase B — the window (chain halts):
4. `./halt.sh` — stops every seat service.
5. `./read-tips.sh` — queries each seat's executed tip; writes
   `g2-window/anchor.env` (`ANCHOR_HEIGHT`, `ANCHOR_DIGEST`).
6. `./apply.sh` — per seat: `finalize` (bind share to keys.json),
   write new `node.toml` (new peer table + `restart_anchor`), move
   `/data/consensus` journal aside (preserve, don't delete).
7. `./start.sh` — start anchor-seat first, then the rest.

Phase C — verify:
8. `./verify.sh` — confirms all seats finalizing, checks the new seat
   appears in leader rotation, confirms `peers` count = n−1, and that
   the retired seat cannot rejoin.

## Rollback

Journals are moved to `/data/consensus.g2-bak` (not deleted). To roll
back: restore old `keys.json` + `node.toml` + journal dir on every
seat, remove `restart_anchor`, restart. Rollback only works if *all*
seats roll back together — never mix old and new polynomial.
