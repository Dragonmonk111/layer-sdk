# JunoClaw G2 Validator Onboarding Pack

Status: G2-KICKOFF (filled 2026-10-14). This pack extends
`docs/VALIDATOR_ONBOARDING.md` (devnet basics) and is what we send
to G2's first external operators.

Audience: external validator operators (G2 shortlist: InfraSingularity,
Anatoly, ffern). They run infrastructure on other networks; they have
not run JunoClaw before. Voice: precise, honest, zero hype.

---

## 1. What you are joining

JunoClaw is a sovereign Rust L1: Commonware simplex BFT with BLS12-381
threshold finality and MAYO2 post-quantum hybrid certificates on every
block. Not Tendermint, not stake-weighted: the validator set is fixed
per phase, every validator has equal weight, and finality is a
threshold signature. There is no staking and no slashing at the
consensus layer — safety comes from the threshold, and ops
discipline replaces economic discipline. That is why we audit seats
for real before they go live.

G2 is the public testnet phase. G1 (the closed 4-seat ceremony
testnet) ran 48.2 h across four independent operators: 361,709 blocks
finalized at ~0.5 s/blk, zero state divergence, hybrid
BLS12-381 + MAYO2 certificates on every block. One 106-minute wedge
occurred — a shared ISP failure domain across two seats — and the
full post-mortem is published; we report it because it is the kind of
fact you would want to know before running a seat.

Set changes are rehearsed, not theoretical: our R9 devnet rehearsal
exercised a coordinated halt + fresh threshold deal for both a seat
addition (4→5, including a cold joiner that backfilled from genesis)
and a removal (5→4, with the retired seat confirmed isolated). The
procedure and results are in `docs/R9_SET_CHANGE_RUNBOOK.md` —
ask us for the log.

## 2. What we require (the seat bar — we check all of it)

1. **Hardware envelope (minimum):** dedicated LXC/VM — 2 vCPU,
   4 GiB RAM, 20 GiB SSD. Measured reality on a live proposer:
   ~2.2 GiB RSS, chain DB growth ~75 MB/day at full soak rate.
   Recommended: 4 vCPU / 8 GiB / 50 GiB.
2. **Clock discipline:** chrony or systemd-timesyncd holding skew
   **< 2 s**. The chain rejects proposals stamped more than 2 s ahead
   of the receiving validator's clock (MAX_FUTURE_DRIFT = 2 s). Our
   G1 audit caught one host 83 s behind NTP and one at 73 µs — we
   check before you join and periodically after.
   Pre-flight: `chronyc tracking` (or `timedatectl`) — RMS offset
   should be well under 1 s.
3. **Stable uptime + a named ops contact** we can reach when your
   seat alarms. The chain tolerates f=1 of n, but a silent seat is
   a dead leader slot — every view that lands on your seat times
   out and costs the network ~3-5 s.
4. **Run the official binary** from this pack (below). No forks of
   the config bundle; peer lists are literal IPs today and every
   entry must match the agreed set.

## 3. Keys — what you hold, what we never hold

Each validator has ONE `keys.json` with:
- **Ed25519 identity key** — your authenticated P2P identity
- **MAYO2 key** — your post-quantum signing key
- **BLS12-381 share** — your threshold signing share

You generate the Ed25519 + MAYO2 keys **locally; they never leave
your machine**. The coordinator deals your BLS share once, over an
encrypted channel, and deletes its copy after delivery. A certificate
requires a BLS threshold AND a MAYO2 quorum — the coordinator alone
can never forge one. A dealer-free DKG replaces coordinator dealing
before mainnet (it is on the public roadmap).

Never share `keys.json`. Losing it means losing your share — back it
up once, offline.

## 4. Install — one command

The G2 binary is tag `v0.6.0-rc10` (includes the `restart_anchor`
consensus mechanism that makes coordinated set changes safe, plus the
idempotent journal-prune fix). The per-seat bundle ships a
`MANIFEST.sha256` — verify the binary against it before first run.

```bash
# Option A — release binary (recommended; bundle link in your pack)
# sha256sum slay3rd → must match MANIFEST.sha256

# Option B — Docker
docker build -f docker/Dockerfile.slay3rd -t junoclaw/slay3rd .

# Option C — build from source (Rust 1.97+, clang/g++/cmake)
git clone -b v0.6.0-rc10 https://github.com/Dragonmonk111/layer-sdk.git junoclaw-chain
cd junoclaw-chain
cc -c -o /tmp/shim.o docker/shim.c
RUSTFLAGS="-C link-arg=/tmp/shim.o" cargo build --release -p slay3rd --features rocksdb
```

## 5. Configure — the config bundle

You receive: `node-<i>.toml` (your index, your peers), `keys.json`
(your share), `genesis.json` + `shared.json` (public — share freely),
and the ops-contact registration form.

```toml
validator_index = <assigned in your bundle — it is your position in
                   the sorted Ed25519 order, NOT your IP order>
chain_id       = "junoclaw-1"
p2p_listen     = "0.0.0.0:7001"
grpc_listen    = "0.0.0.0:9090"      # firewall to coordinator until public endpoints ship
bls_key_path / identity_key_path     # -> your keys.json
data_dir / genesis_path              # per bundle
[[peers]]                            # every other validator's ed25519 pk + literal IP
```

One R9 finding worth knowing: `validator_index` follows the
lexicographic sort of Ed25519 public keys across the set — when a seat
is added, everyone's index can shift. Your bundle's `node-<i>.toml`
is generated by the ceremony tool with the correct index; never reuse
an index from an earlier window.

Rules: `chain_id` must match the set exactly; `validator_index` must
match your key share; peers must be the literal IPs from the bundle
(no DNS until the resolver work lands).

## 6. Join — you sync, not replay

You are joining a RUNNING chain. Your node catches up via payload
backfill over authenticated P2P (`ensure_executed` walks parent links
and fetches+executes missing history in order); BLS-certified
state-sync snapshots are the fast path when within range.

Rehearsed reality (R9, devnet): a cold seat with empty data dir joined
a running set, backfilled ~4,900 heights through the resolver, reached
tip, and proposed blocks — verified end-to-end. When the set changes,
all seats restart from a shared `restart_anchor` (agreed height+digest
in the bundle) so every validator proposes from the same parent.

We run this exact path in rehearsal before any external seat joins —
it is our gate, not yours; ask us for the runbook.

## 7. Run + verify

```bash
slay3rd /path/to/node-<i>.toml     # or the systemd unit from the bundle
```

Healthy join: state-sync adoption log lines, height converging to tip
(gRPC `layer.lightclient.v1.Query/LatestHeight`), then your partial
signatures appearing in aggregated certificates. If you are up but
not signing, check in order: `validator_index` matches, peers
reachable on 7001, `chain_id` matches, clock under 2 s.

## 8. Operations

- **Alarms:** we monitor your seat's gRPC + log signature; when it
  alarms, we page your registered ops contact. Same in reverse — your
  ops channel is set up at onboarding (a dedicated contact path agreed
  with the coordinator; a public validator channel is PLANNED — the
  agent-relay UI is being re-faced at buzz.junoclaw.xyz).
- **Upgrades / set changes:** the set is static per window; changes
  ship as a coordinated halt, fresh threshold deal, and restart at an
  agreed `restart_anchor` height+digest — all seats at once, never one
  at a time (mixed-version or mixed-set P2P breaks). The full ceremony
  is in `docs/R9_SET_CHANGE_RUNBOOK.md`; we schedule windows with you
  in advance and your `restart_anchor` value arrives in the bundle.
- **Restart:** safe any time; your node re-syncs (state-sync or
  backfill) and rejoins. A missed window is a skipped view, not a
  slash — there is no slashing.
- **Failover — STRICT RULE:** never run two instances carrying the
  same `keys.json` concurrently. Two live copies of one share =
  double-signing = provable equivocation. Failover must be
  active/passive with hard fencing (the standby may only start
  after the primary is confirmed dead). If your standard posture is
  multi-region active/active clusters, this seat runs differently —
  one live signer, always.
- **Backups:** `keys.json` (offline, once). `data_dir` is rebuildable.

## 9. What we check on our side (the seat audit)

Before your seat goes live: clock audit (chronyc/timedatectl output),
resource headroom (nproc/free/df), single-tenant confirmation, ops
contact reachable test-page. Periodically after: the same.

## 10. FAQ

- **Mainnet date?** No date until the gates hold (sustained finality
  under churn, cross-chain light-client demos, PQ hardening). The
  milestone roadmap at junoclaw.xyz tracks LIVE / DEVNET / PLANNED.
- **Token?** Fixed supply 54.66M, no inflation, no sale. Validators
  earn fees. The airdrop (juno-1 stakers) is a mainnet event.
- **What if my node goes down?** The chain continues (f=1 tolerated).
  You rejoin via state-sync. Liveness needs 2/3+ of the set online.
- **Why literal IPs?** G1 finding: stale DNS/lease addresses wedge
  peering silently. DNS support lands with the resolver work.
- **Monitoring surface?** gRPC on 9090: LatestHeight, Block(h) with
  the certificate (you can verify every block yourself with
  tools/verify-cert), tx service, Cosmos-standard queries.

## 11. Pack contents checklist (what we send)

- [ ] This document
- [ ] `node-<i>.toml` bundle + `genesis.json` + `shared.json` +
      `restart_anchor` for the join window
- [ ] `keys.json` (encrypted channel; you verify MANIFEST.sha256)
- [ ] Ops-contact registration form
- [ ] Endpoints: explorer.junoclaw.xyz · api.junoclaw.xyz
- [ ] Milestone roadmap: junoclaw.xyz (LIVE / DEVNET / PLANNED labels)

Questions → your onboarding contact directly (dedicated ops channel
per-seat; a public validator channel is PLANNED).
