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
| D1 | `force_state_sync` config flag + startup diagnostic when `[state_sync]` is configured but incompatible stored state exists | A node with stale genesis state + state_sync config either syncs or fails loudly, never silently runs on genesis |
| D2 | Peer-mesh diagnostics — expose authenticated peer count/identity (peer-ready) in logs or a status endpoint | Operator can answer "am I meshed?" without `ss`/journalctl |
| D3 | rc11 release (D0+D1+D2) | Tagged release, install.sh updated |
| D4 | Rolling validator upgrade to rc11 | All 5 seats on rc11, no finality gap |
| D5 | Future ceremony UX: encrypt `dealing-priv-*` files to recipient pubkey (removes coordinator trust ceiling) | Design + impl in `generate-testnet-keys` |

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
| F1 | MAYO audit readiness — per `docs/MAYO-AUDIT-PLAN.md`: pin sriracha-mayo version, publish KAT vectors + build instructions, scope the audit surface | Audit-ready package handed to auditors |
| F2 | DAO DAO P0 audit — devnet deploy of dao-contracts suite, instantiate minimal DAO | Gap list produced; shared with DAO DAO team (doc: `docs/DAO-ON-JUNOCLAW.md`, on HackMD) |
| F3 | Funding structure — operator infra costs, grants, treasury design | Concrete proposal; feeds Treasury SubDAO |
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
