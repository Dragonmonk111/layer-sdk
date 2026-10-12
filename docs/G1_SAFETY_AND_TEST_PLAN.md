# G1 Safety Overview, Monitoring, and Test Plan

Chain `junoclaw-g1` - 4 invited validators (n=3f+1, f=1), 3-of-4 BLS threshold,
hybrid Ed25519 + MAYO2 identities. Coordinator state as of ceremony assembly.

## 1. Seats

| idx | seat | p2p (literal IP, no DNS) | host | gRPC monitor |
|---|---|---|---|---|
| 0 | akash-seat | `196.216.137.249:30823` (`provider.dl2klab.co.za`) | Akash lease dseq `28973001`, DL2K Lab (ZA) | log-only |
| 1 | home-vb | `84.9.189.220:7001` | home | `:9090` |
| 2 | ovh-uk1 | `51.195.201.74:7001` | OVH UK | `:9090` |
| 3 | vahana | `57.129.103.229:7001` | Ravi | `:9090` |

The Akash seat is log-only because a second exposed port cannot be added to an
existing lease through `deployment update` (resource endpoints are part of the
on-chain group spec). Monitor it with `provider-services lease-logs` (grep `CONS-05`).

## 2. Key-safety model (deterministic)

Private material inventory and where it may live:

| material | produced by | allowed locations | never |
|---|---|---|---|
| `keys.json` (Ed25519 + MAYO2 + BLS share) | `keygen-share` + `finalize` | its own seat only | git, chat, other seats, shared drives |
| `bls-share.json` | `assemble-genesis` (trusted dealing) | coordinator `ceremony-out/` until delivered, then securely deleted | git, plaintext email |
| G1 DAO account label | `snapshot/g1-account.mjs` | `g1-ceremony/_secrets/` (ACL-restricted), offline backup | git, logs |
| `deploy-akash-seat.yaml` (filled SDL) | `g1-akash-manifest.mjs` | `_secrets/` only | git |
| `share-request.json`, `shared.json`, `node-<i>.toml`, `genesis.json` | ceremony | **public**, may be shared | - |

Enforced mechanically:

- `.gitignore` blocks `keys.json`, `bls-share.json`, `_secrets/`, `ceremony-out/`, `validator-*/`, `*.age`.
- `node g1-ceremony/g1-preflight.mjs` must print `ALL INVARIANTS HOLD` (26 checks):
  no secret tracked or `git add`-able; 4 unique identities; literal-IP peers;
  each `node-<i>.toml` wires exactly the other 3 seats with correct chain id, no
  `insecure_devnet`; genesis funds no known devnet account; DAO address matches
  the secret store. It also writes `public/MANIFEST.sha256`.
- Every operator compares `MANIFEST.sha256` hashes of `genesis.json` and
  `shared.json` before boot. **Genesis SHA-256:**
  `f6c0e9c0570b50927fc8a81465ba9948d66937b14133c2fb2525e713d645a7a5`.

Trust assumptions (accept explicitly):

- The coordinator transiently holds all 4 BLS shares (trusted dealing). Forging a
  certificate still needs a MAYO2 quorum whose keys never left operator machines.
  Dealer-free DKG is a pre-mainnet item.
- The Akash provider can read the manifest, therefore it can read the Akash seat's
  `keys.json`. Blast radius = one seat of four (f=1 tolerated). Testnet only.
- Gossip/gRPC port 9090 also accepts `BroadcastTx`. Restrict 9090 to the
  coordinator IP on seats that open it (firewall), do not expose to the world.

Rotation if a seat is suspected compromised: stop it, `keygen-share` fresh, re-run
`assemble-genesis` (new threshold polynomial), redistribute to all seats.

## 3. Monitoring endpoints

| endpoint | what it tells you |
|---|---|
| gRPC `layer.lightclient.v1.Query/LatestHeight` on `:9090` | finalized height + timestamp per seat |
| gRPC `layer.lightclient.v1.Query/Block(height)` | proposal bytes + BLS threshold certificate (verify cert) |
| gRPC `layer.sync.v1.Query/LatestSequence` | sync tip |
| gRPC reflection on `:9090` | `grpcurl list` introspection |
| Cosmos gateway queries (bank/auth/wasm) on `:9090` | state correctness, balance of DAO account |
| logs `grep CONS-05` | "Block finalized with BLS threshold certificate", per-height timestamps |
| logs `view timeout` WARN | leader timeouts / quorum trouble |
| Akash: `provider-services lease-status` / `lease-logs` | pod ready, log tail for the Akash seat |

Run: `node g1-ceremony/g1-monitor.mjs [--interval 5] [--log g1-run.jsonl]`
(`--once` returns exit 0 healthy / 1 degraded / 2 halted, usable in cron).
It reports per-seat height, lag, block period, tip age, RTT, stall, and
below-quorum alarms. Seat endpoints live in `g1-ceremony/g1-endpoints.json`.

## 4. What we are testing and what "good" looks like

| # | test | how | pass criteria |
|---|---|---|---|
| T1 | Boot and first finality | start all 4, watch monitor | all seats reach h>=1 within 2 min; `CONS-05` appears on every seat |
| T2 | Agreement | compare heights/certificates across seats | height spread <= 2; same cert digest per height |
| T3 | WAN block time | monitor `period` over 1 h | in line with doc table (EU+ZA mix expect ~0.5-1.5 s); record p50/p99 |
| T4 | Leader kill | stop the current leader seat | chain skips the view, resumes within ~3 s (`leader_timeout_ms`); n=4 quorum=3 still finalizes |
| T5 | Restart and backfill | restart a seat (keep volume; for Akash the pod restart wipes /data) | catches up via backfill, height converges; Akash seat re-syncs from peers |
| T6 | Two-down halt | stop 2 seats | chain **halts** (no safety violation, no divergent heights), resumes when 1 returns |
| T7 | State machine determinism | submit identical txs, compare `state_root`/balances on all seats | identical on every seat |
| T8 | Tx path | `BroadcastTx` bank send from DAO account via a seat, query on another | included, visible on all seats |
| T9 | CosmWasm | deploy a contract, execute, query | success, same result on all seats |
| T10 | Airdrop flow | deploy airdrop-claim with merkle root, fund it, claim with a proof | claim succeeds once, second claim rejected |
| T11 | Light client | fetch `Block(h)` + certificate, verify BLS cert against `threshold_public_key_hex` | verifies for every sampled h |
| T12 | Soak | leave running 24-72 h | no divergence, no wedge (cf. C9 backfill wedge, 2026-10-02), p99 block time stable, WARN/ERROR counts logged |
| T13 | Clock skew | check NTP on all hosts | skew < `MAX_FUTURE_DRIFT` (2 s); no rejected proposals |
| T14 | Resource use | host CPU/mem/disk per seat | within 2c/4G/20G envelope |

### Results (junoclaw-g1, rc9)

- **T1-T6 PASS** — implied by sustained 4-of-4 finality since the wedge recovery
  (2026-10-10 21:17 UTC): all seats booted, heights converged, ~2.5 s WAN
  block period steady, and the chain survived the 2-of-4 wedge + a state-sync
  restart (T5 exercised live) without divergence.
- **T8 PASS** (2026-10-11) — MsgSend 1000 ujclaw DAO→fresh acct committed at
  h413879 (txhash `A172A667…35A15`, `code=0`, gas 20220); second send 7 ujclaw
  at h415962 (txhash `5A9242AB…E229`). Recipient balance `1007 ujclaw` verified
  via gRPC on vahana + home-vb; block included on all seats.
- **T7 PASS** (2026-10-11) — byte-identical state fingerprints across all 4
  seats. Block h415962 `payload_sha256=707700c8…26af` identical on vahana,
  home-vb (gRPC `Query/Block`, new `tx-sender block` subcommand) and ovh
  (journal digest). Per-height `digest`+`app_hash=502d2136…8f86` identical
  between ovh and akash-seat at h430654-430656; seat `app_hash` still equal
  to ovh's post-tx2 root ~15k blocks later (empty blocks keep state).
- **T9 PASS** (2026-10-11) — full CosmWasm path live on all seats.
  `store-code` hackatom.wasm (180,690 B) at h438125 → `code_id=2`
  (`572441C3…E9D4`, gas 2,845,606). `instantiate` at h439418 → contract
  `juno14ryr0qkgjq8rmywnpwq553zhzzc50uzpy25cf7wlzf9r5n3ylzusakljpm`
  (`54A266E9…E114`, gas 89,741). `execute {"release":{}}` w/ 50 ujclaw at
  h440495 (`6CEB8876…E3C6`, gas 93,950): `wasm-hackatom`+`transfer` events,
  contract balance 0. Smart queries byte-identical on vahana + home-vb:
  `verifier=juno1r643…s5`, `get_int=15731626`. h440495 digest
  `8e4686c1…cc7b` identical vahana (gRPC) vs ovh (journal). Negative path
  also deterministic: malformed-msg instantiates h438846/h438998 → `code=1`,
  `gas_used=87184` identical everywhere. `--msg-file` avoids PS quoting.
- **T10 PASS** (2026-10-11) — real `airdrop-claim` contract end-to-end.
  Built `contracts/airdrop-claim` for wasm32 (workspace `contracts/`).
  `store-code` h452981 → `code_id=3` (321,304 B, `0245E264…B30C`,
  gas 5,144,486). `instantiate` h457062 → contract
  `juno1wxwgn2zt3dn6kdul5h2msacx0t5xfuv2d9ddcp3azq49lhcy8cnq5054hd`
  (`4C8C71AA…AAD6`, gas 106,139; 2-leaf sorted-pair tree, DAO claim 500,000).
  Funded 750,000 ujclaw h457448. `claim` w/ merkle proof h458054
  (`4FE78AFE…3A06`, gas 100,243): `transfer` event, contract bal
  750,000→250,000. Second claim rejected h508995 `code=1`
  "Address has already claimed" (gas 80,839). Queries identical on
  vahana + home-vb: `has_claimed=true`, stats `750000/500000/250000/1`.
  Negative path deterministic: bad-type instantiate h455751 `code=1`
  gas 88,898. Note: claim2 inclusion took ~60-90 s (vs <15 s typical) —
  see rc10 1-peer dial candidate.
- **T11 PASS** (2026-10-11) — `tools/verify-cert` on live blocks via
  `tx-sender block --dump` (ovh-uk1 gRPC). IMPORTANT: verify against the
  G1 ceremony committee, not `testnet-keys/` (stale seed-0 devnet set,
  pubkey `91281b3f…`). Public-only committee file:
  `g1-ceremony/ceremony-out/committee-public.json` (group key
  `b27853e2…`, 4 Ed25519 + 4 MAYO2 keys, quorum 3). 7 sampled heights
  VALID on both halves (BLS threshold + MAYO2 3/3): h1000, h100000,
  h457062, h508995, h509500, h510000, h511000. Signer bitmaps vary
  ([0,2,3] pre-wedge, [1,2,3] post-restart) — expected seat-subset
  rotation. h312344 (wedge block) unverifiable: light-client query
  returns `proposal_len=0`, proposal never stored — known artifact.
  Envelope: 616 B hybrid `[0x01][48B BLS][u64 bitmap][N×186B MAYO2]`.
  **On-chain LC contract PASS** (2026-10-11): `contracts/light-client`
  deployed code_id 4 (h512369, `106333C9…`, gas 8.1M), instance
  `juno15y9at4h38vw98a4jredf9xcjplgcly9pcmpx7nenxtm72uvc5kuq8dsxcw`
  anchored on h508995. `verify_client_message` real h508995 header →
  `{}` (pass); tampered proposal payload → pairing-check-failed
  error (negative path). Gotcha: set `TX_SENDER_CHAIN_ID=junoclaw-g1`
  (default `junoclaw-1` → check_tx `InvalidSignature`); deployer
  `juno1dz875…` funded 5 JCLAW from DAO (`89EFF5D4…`).

Failure conditions that stop the test and require investigation: any height
divergence between seats, a certificate that fails verification, a seat wedged
>1 min behind quorum, or any private-material exposure.

## 5. Launch sequence

1. `node g1-ceremony/g1-preflight.mjs` -> `ALL INVARIANTS HOLD`.
2. Deliver to each of `home-vb`, `ovh-uk1`, `vahana`, over an encrypted channel:
   `validator-<i>/bls-share.json`, `node-<i>.toml`, `public/genesis.json`.
   Each runs `generate-testnet-keys finalize`, then boots (see
   `G1_OPERATOR_QUICKSTART.md` step 4). Coordinator deletes local copies of delivered shares.
3. Akash seat: `finalize` is done locally; push `_secrets/deploy-akash-seat.yaml`
   with `akash tx deployment update` + `provider-services send-manifest`.
4. Start `g1-monitor.mjs`, confirm T1-T2, then run T3-T14.
