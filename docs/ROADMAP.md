# JunoClaw Roadmap — Post-G2 Master Plan

*Written 2026-10-16, after the successful G1→G2 cutover.*
*Current state: chain live at n=5 / 4-of-5 threshold, group key
`b27853e2…` preserved across the set change, all seats external-or-
independently-operated except home-vb/ovh-uk1 (ours).*

## Phase C — Verify the new set end-to-end (immediate)

| # | Item | Proof of done |
|---|------|---------------|
| C1 | Chain health under new set | Continuous `Block finalized`, all 5 seats appearing in cert signer bitmaps, no view churn |
| C2 | Public services | RPC answers latest height; faucet drips; indexer/archiver tracks tip; explorer + status page render current data |
| C3 | IBC client continuity | Osmosis-side 08-wasm BLS client verifies post-cutover headers (group key preserved → should be seamless; verify on a real header) |

## Phase D — Hardening from G2 lessons (this week)

| # | Item | Proof of done |
|---|------|---------------|
| D0 | ~~`min_anchor_agree` on advertised snapshot root~~ | ✅ done — `b54762f`, quorum-gated halted-chain fallback, 9/9 tests |
| D1 | ~~`force_state_sync` config flag + startup diagnostic~~ | ✅ done — `dc44099`; **proven live 2026-10-17**: home-vb wedged 4.5h, `force=true` wiped stale state, donor-quorum sync at 2,064,700, back voting in minutes |
| D2 | ~~Peer-mesh diagnostics~~ | ✅ done — `dc44099`; live on home-vb (`peer mesh: reached 4/4`) — but see D6 (metric blind spot) |
| D3 | ~~rc11 release (D0+D1+D2)~~ | ✅ done — `v0.6.0-rc11` tagged + published on layer-sdk releases, assets checksum-verified |
| D4 | Rolling validator upgrade — home-vb soaked rc11 overnight; external seats go straight to **rc12** (rc11 + D6/D7) so operators upgrade once | All 5 seats on rc12, no finality gap |
| D5 | ~~Encrypt `dealing-priv-*` files to recipient pubkey~~ | ✅ done — ephemeral X25519 + XChaCha20-Poly1305 sealed to recipient Ed25519 identity; readers auto-accept legacy plaintext; `--plaintext-priv` escape; 6/6 unit tests |
| D6 | ~~**pending-capacity catch-up deadlock**~~ — `pending_payloads` caps at 16,384; unsolicited far-tip pushes (within `within_observed`) fill it; solicited replies for the tip+1 link get *rejected* → execution permanently wedged, channel-3 mailbox goes `Full(..)`. Reproduced live on home-vb 2026-10-17 (4.5h, recovered only via state-sync) | ✅ done — `d275579`: unsolicited admissions stop at `MAX - 1024` reserve; solicited replies may evict the furthest pending entry; tests extended |
| D7 | ~~mesh metric blind spot~~ — it only counted payload-broadcast deliveries (proposer path); a stalled/non-proposing node read `0/4` falsely | ✅ done — `d275579`: metric now includes inbound authenticated messages + fetch/reply sends |
| D8 | **Silent process deaths under supervision** — home-vb 2026-10-19: node exited clean (ExecMainCode=0) every ~2-3min under systemd while identical foreground/detached runs survived 45min+. Root cause unconfirmed; panic="abort" hid all crash evidence. Partially addressed in `4d2eb41` (panic hook + non-zero state-sync exit); kill source still open | detached+cron guard interim; needs the next-death backtrace |
| D9 | **Requested-height markers don't survive restart** — `requested`/`requested_heights` maps are in-memory; a restart mid-catch-up wipes solicitation state so late replies count as unsolicited. Home-vb wedged 2026-10-19 (tip 2,434,187) on a d275579 binary — either this gap or payloads already pruned on all donors | persist requested heights, or re-request missing finalize links on boot |

## Phase E — Next validator windows

Applicants in queue: **ffern**, **Andrei** (ex-Juno mainnet validator),
plus inbound interest — target 6–7 seats.

| # | Item | Proof of done |
|---|------|---------------|
| E1 | Collect share-requests (`generate-testnet-keys` output) from candidates | One `share-request.json` per candidate, MAYO attestation verified |
| E2 | 5→N reshare ceremony — `--previous dkg-output-1.json` | `dkg-observe` group key still `b27853e2…`; all N finalize |
| E3 | Coordinated cutover (same flow as G2, now rehearsed) | New set finalizing; cleanup checklist run |
| E4 | New seats' node.tomls ship with `[state_sync]` + donor ports pre-opened | No manual patch round like G2 needed |

## Phase F — Governance / funding / audit track (parallel)

| # | Item | Proof of done |
|---|------|---------------|
| F1 | MAYO audit readiness | ✅ package done — `docs/MAYO-AUDIT-PACKAGE.md`: sriracha-mayo 0.0.4 pin + checksum, KAT corpus + regen path, audit surface, vendored-storage diff scope |
| F2 | ~~DAO DAO P0 audit~~ | ✅ PASSED 2026-10-17 — full suite live on testnet (code_ids 5–10); nested instantiate 4-deep; propose→vote→execute round trip done; gap list in `DAO-P0-RUNBOOK.md` §5 |
| F3 | Funding structure — operator infra costs, grants, treasury design | ✅ draft done — `docs/FUNDING-STRUCTURE.md`; feeds Treasury SubDAO |
| F4 | Juno Agents DAO + Validator Ops SubDAO instantiation (post-F2) | Live DAO; next set-change ratified on-chain |

## Phase G — Mainnet-readiness continuous work

| # | Item | Proof of done |
|---|------|---------------|
| G1 | jc-explorer deploy (built, committed) | Public explorer serving tx/address queries at tip |
| G2 | Ops channel decision (buzz re-face vs Telegram) | Decision + migration done |
| G3 | Monitoring hardening — jc-status webhook live, alerting wired | Alerts fire to ops channel on stall |
| G4 | IBC hardening impl (`docs/IBC-HARDENING.md`) | Tendermint-side verifier + client group-key rotation landed |
| G5 | Airdrop/genesis allocation design for mainnet | Documented, reviewable |
| G6 | In-protocol validator rotation — the endgame: DAO-ratified set changes at epoch boundaries | Design doc; no more manual ceremonies |

## Ceremony lessons carried into all future windows

15 lessons in agent memory (`global_rules.md`): clean-serve-dir rule,
version-stamped artifacts, no remote globs, gitignore-first, staged key
swaps, backup verification, state_sync-on-empty-storage, donor-root
quorum, "port open ≠ peer connected", conflicting-nullify noise,
explicit `halted_chain_anchor` authorization on record, `>>` logging,
WSL expansion gotchas, .g2-bak journal parking, Telegram attachment
losses.
