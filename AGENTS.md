# JunoClaw — Agent Operating Rules

## Ceremony secret handling (NON-NEGOTIABLE)

1. **Never serve a directory over HTTP without auditing its contents first.**
   Create a dedicated clean directory (e.g. `~/serve/`), copy ONLY the
   public file into it, and serve that. Never point a server at a
   working directory that may contain `dealer.seed`, `keys.json`,
   `dealing-priv-*`, or key backups. (Near-miss: 2026-10-16, G2 reshare.)

2. **Never open a firewall port before the served directory is verified
   clean.** Order is: stage clean dir → start server → verify listing
   → then open port. Verify externally from a second host, not just
   localhost.

3. **Keep the access log.** A shared `> log` redirect overwrites the
   previous server instance's log — use `>>` so request evidence of
   earlier instances isn't destroyed.

4. **`keys.json` never leaves the operator's host.** `dealer.seed`
   stays on the dealer's host until `dkg-log`, then is deleted. Only
   `dealing-priv-*-for-*.json` and `dealing-pub-*.json` travel, and the
   priv files go only to their named recipient.

5. **Broad globs on remote dirs are dangerous.** `scp host:dir/*`
   pulled `dealer.seed` to the coordinator once. Copy named files only.

6. **Git:** `g2-window/{ceremony,dealers,g1-keys,share-requests,acks}/`,
   `dealer.seed`, `keys.json`, `bls-share.json` are gitignored. Never
   `git add` a ceremony dir wholesale. Check `git status` + search for
   secret filenames before every commit.

7. **Reshare, never re-deal:** live set changes use `dkg-info → dkg-deal
   → dkg-ack → dkg-log → dkg-observe → dkg-finalize`, preserving the
   group key (IBC trust root). `assemble-genesis` creates a NEW group
   key — never use it on a live chain.

8. **Known trust ceiling:** in the current file-relay flow the
   coordinator sees plaintext per-player dealing messages in transit.
   Before mainnet, encrypt each `dealing-priv-*` file to the
   recipient's Ed25519 key (X25519) so transit files are ciphertext.

## Live-validator change control (2026-10-21 g1 halt)

9. **No host, network, or config changes on a LIVE validator outside an
   announced maintenance window.** The Oct 21 halt (7h at height
   2,631,739) was triggered by a mid-incident host migration on home-vb
   that left inbound P2P half-broken → view churn → the variant-
   execution bug fired. Any infra change goes through a verified
   canary first, and lands in a declared window only.

10. **Verify inbound P2P end-to-end after ANY network change.** A
    VBox pf rule existed in config but never bound the port — "rule
    configured" ≠ "port forwarded". Test inbound from a second host,
    not just `ss -ltn` locally.

11. **Downtime alerting is part of the node.** The stall ran 6+ hours
    before anyone paged it. Minimum bar: finalization-lag alert
    (>10 min without a `Block finalized`) + cross-seat digest parity
    check on any suspected stall before assuming "chain fine".

12. **Digest identity is consensus-critical even when app state is
    equal.** Two payloads at the same height with identical app_hash
    but different bytes are DIFFERENT blocks. Compare digests, not
    just heights/state roots, when auditing seats.

## Ops tools build + deploy (jc-status, jc-explorer)

13. **Build on GitHub Actions, not locally.** `gh workflow run
    build-ops-tools.yml --ref commonware -f package=jc-status` builds +
    tests on Ubuntu 22.04 (matches OVH glibc 2.35) and uploads the
    binary with a sha256. Download with `gh run download <id>`.
14. **Deploy order on OVH:** verify sha256 on host → smoke-run on a side
    port (e.g. 127.0.0.1:18090) → back up `/usr/local/bin/<tool>.bak-<date>`
    → install → restart → check `/api/status`. jc-status flags/env live in
    `/etc/systemd/system/jc-status.service.d/alerts.conf`
    (`JC_ALERT_CMD=/opt/buzz/jc-alert-post.sh`, `--seat-names` in
    validator-index order — update it at every reshare).
15. **Telegram alerts = config only:** add `JC_ALERT_WEBHOOK=https://api.telegram.org/bot<token>/sendMessage`
    + `JC_ALERT_CHAT_ID=<id>` to that drop-in. Token never in the repo.

## J-Claw Relay ops (learned 2026-10-29)

16. **Operator packet v5 is deferred — bundle it with the rate-limit/DoS
    tranche.** v4 (`ad14287e` binary) is the current sendable artifact;
    OVH runs `ffb751f0` ahead of it by two cosmetic nits (localhost
    u-tag normalization, NIP-11 description). Do NOT invite further
    operators (Anatoly, PostHuman, …) until v5 ships WITH the DoS
    tranche: message-size caps, per-connection subscription/rate
    limits, AUTH-before-write policy decision, WS-REQ read policy
    (gossip peers are currently ungated by design — IP restriction is
    the gate). No public exposure before that tranche lands.

17. **Gossip peering topology:** `--peer` is pull-only ws:// and serving
    is ungated — NEVER bind the relay listener publicly. Pattern that
    works: relay stays `--listen 127.0.0.1:7070`; a socat door
    (`systemd claw-peer-door.service`, `WAN_IP:7071 -> 127.0.0.1:7070`)
    + ufw source-restrict to the peer's IP. OVH peers at
    `ws://57.129.103.229:7071` (vahana).

18. **claw-checkpoint signs via bls-relayer — the signer MUST match
    `--author`.** Without `--jc-key-file`, jc-execute falls back to the
    embedded deployer key (juno1dz875zg…): txs commit fine but
    list_by_author(--author) never matches → FAILED commit loop +
    duplicate orphan entries every retry. Key flows via
    JUNOCLAW_KEY_HEX env injected from a root-600 hex file
    (`/opt/claw/state/dao-key.hex`, derived from /opt/dao-keys/dao-key.pem)
    — never argv. Also: `list_by_author` returns entries in ASCENDING
    ID-HASH order, not chronological — any single-page verify can miss;
    paginate via start_after.

19. **Moultbook orphan entries (2026-10-28/29):** ~20 under the deployer
    key (claw-checkpoint wrong-author posts) + ~19 identical-commitment
    reposts under juno134zsp2 (buzz transcript-anchor.sh state-write
    perm bug — fixed via chown). Decision: LEAVE them — visible
    evidence trail; Redact cleanup is available later if wanted.

20. **Build claw-* linux binaries on bullseye, not bookworm.** OVH is
    Ubuntu 22.04 (glibc 2.35); rust:1.98 (bookworm, glibc 2.36+)
    binaries can fail `GLIBC_2.39 not found`. Use
    `rust:1.98-bullseye` + CARGO_TARGET_DIR=target-linux-bullseye.
    (Longer-term: wire claw-* into build-ops-tools.yml CI like rule 13.)

21. **Run-script edits: write the file, never sed multi-line inserts.**
    `sed 's/…/…\\n…/'` over SSH produced a literal `\n` + stray arg in
    run-relay.sh/run-checkpoint.sh twice. `cat <<EOF > file` the whole
    script instead.

## Juno governance — Juno Agents DAO prop A57 (2026-10-29)

- DAO `juno18k65at7fkf8elhece0fnhsvuxggqg6cved6trp5fyk3lftfn93xsmpeaac`
  (prop module `juno1jar50ltryvzp6axanam3v6gwsxakp2edmrz0n4r7y7h3hcwarp3sm6ccsp`),
  prop A57 "Submit signaling proposal: A New Direction for JUNO".
- Execution: DAO submits /cosmos.gov.v1.MsgSubmitProposal to chain gov
  with 5,000 JUNO initial deposit from a ~9,000 treasury — burns if vetoed.
  Signaling ONLY: no migration, issuance, spending, or shutdown.
- Signal text: move JUNO's financial home to a liquid chain (Solana /
  Robinhood Chain named), liquidity incentives, and "develop a modern
  WASM chain in parallel" with a transition plan.
- **Decision: the JunoClaw agent wallet votes YES.** Rationale: the
  modern-WASM clause is the lane JunoClaw occupies; signaling opens the
  conversation at zero commitment; our seat exists to steer direction.
  Caveat noted: deposit risk rides on the Solana/migration clause, not
  the WASM clause — if that draws veto energy, a narrower WASM-only
  signaling prop is the fallback.
- Strategic frame: juno = testbed / agent sandbox / canary for
  JunoClaw; JunoClaw = the modern wasm chain the direction gestures at.
  Jake revived the Agents DAO at our request — it is the governance
  seat we steer this through. Next steps: vote YES early (quorum 33%),
  watch for the chain-gov prop, then engage on the "modern WASM chain"
  implementation line — JunoClaw named as substrate.

## Positioning — Commonware wave (2026-10-29)

- Patrick O'Grady (commonware founder, ex-Ava Labs/Coinbase) posted:
  "The blockchain products launching over the next few months will blow
  you (and your agent's) mind. AFAICT web3 tx cost/latency (w/ verifiable
  finality) will be less than or equal to web2 api cost/latency by the end
  of 2026."
- He is publicly selling OUR thesis: agent-first blockchain products +
  verifiable finality at web2 speed. Commonware ecosystem launches are
  coming "over the next few months" — being a visible, credible adopter
  now is how we ride it.
- We are a real adopter, not a tourist: our chain runs on commonware
  primitives (Simplex, resolver/p2p), we filed upstream issue #5292 with
  live incident evidence, and we run the only commonware-powered
  agent-governed L1 with hybrid PQ certs + a DAO-gated sovereign relay.
- Wave-riding actions (ordered):
  1. Public X reply/quote from the JunoClaw account — one line, no hype:
     we run this today, live testnet, verifiable finality, agent-governed,
     PQ-signed. Show don't claim.
  2. Medium/relay article gets a wave-aligned hook (agent-native +
     web2-latency finality framing, not "we built a nostr relay").
  3. Foundation letter carries the same framing — verifiable finality is
     the wave, we ship it.
  4. Keep upstream engagement warm — append confirmed #5292 incident
     evidence when Ravi's digest data lands; commonware maintainers
     noticing a production-adjacent adopter is the cheapest credibility
     in the industry.
  5. When their product launches land, be the reference testnet:
     "already running in production-like conditions with external
     validators and independent observers."
- Do NOT: claim endorsement, overclaim latency numbers, or pitch them
  unsolicited — public visible work only.

## Ops rule — no builds on live validators + release infrastructure (2026-10-29)

- **Ravi's call, ratified as policy:** building a release binary on a
  live-validator host is exactly the heavy activity the change-control
  rules forbid. All seats take PREBUILT artifacts only.
- **Release path (now standing):** `.github/workflows/release-slay3rd.yml`
  — tag any commit `slay3rd-<name>` and CI builds the canonical
  `docker/Dockerfile.slay3rd` image, extracts the binary + sha256 into a
  GitHub prerelease, and pushes `ghcr.io/dragonmonk111/slay3rd:<tag>`.
  Operator verify = `sha256sum` vs the attached `.sha256` before deploy.
- First artifacts: release `slay3rd-rc17-soc` (asset
  `slay3rd-rc17-soc-782f791`, sha256 `1d24dde3…e679f4`) and image
  `ghcr.io/dragonmonk111/slay3rd:rc17-soc-782f791`.
- **Builds are reproducible:** our docker build at 782f791 produced the
  identical sha256 to idx3's deployed canary — same Dockerfile + commit
  → byte-exact artifact.
- ghcr packages default private even on public repos — flip package
  visibility in GitHub UI if external pulls are needed.

## idx6 flap evidence — RC17 SOC deployment (2026-10-29)

- Vahana recreated container with: RC17 SOC 782f791 (sha-verified vs
  release asset) + log retention 100m×3 → 200m×12 + prior forensic
  `docker inspect` snapshot (ExitCode=0, OOMKilled=false; RestartCount
  caveat: manual restarts had reset the counter to 0).
- Rollback reference: previous build sha `3f31f245…`.
- Post-flap signature: `certify: payload unavailable` 21→0/5min,
  `verify: rejecting proposal` 26→0/5min, lag 9–16 → 0–1 blocks level
  with donor tip. New observability firing: `verify: resolved allow`,
  `certify: resolved`, peer-named `Payload relay` reason lines, mesh
  line naming reached peers.
- **Voting NOT yet resumed at flap+immediate:** two bitmaps (views
  893285/893293) = `11101101` (seats 0,1,2,4,5,7) — idx6 and idx3 both
  absent. If bit appears, wedge was fetch-path end-to-end; if absent
  with lag 0 + verify allow, suspicion moves to vote-signing/delivery
  path (same per-peer mailbox class as #5292).
- `42031ea0` verified live: home-vb canary PID 11751 healthy at tip;
  WSL retired node has no process and guard cron still commented — no
  duplicate identity. A `peer mesh: only 1/7 reached [42031ea0]` line
  means the mesh metric tracked ONE reached peer (idx3), not that idx3
  is missing — mesh rebuilds slowly post-restart.

## Juno governance — A58 supersedes A57 (2026-10-29)

- A57 PASSED the DAO vote but execution FAILED: the text-only chain
  proposal carried empty `metadata` — chain gov requires nonempty.
- **A58 = corrected resubmission, same intent:** identical signaling
  text/deposit/proposer/zero executable msgs; `metadata` now links to
  A57's DAODAO page. Validated by read-only mainnet simulation
  (original reproduced the error; corrected produces proposal-create +
  deposit + voting-period-start events).
- **Vote: YES (same decision as A57).** Description is more rigorous —
  veto-burn risk, treasury math, refund policy, "cannot be undone" all
  explicitly disclosed. If A58 also passes, chain-gov submission lands
  a Juno-wide signaling vote: the modern-WASM-chain lane opens publicly.
