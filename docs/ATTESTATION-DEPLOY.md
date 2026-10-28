# seat-attestation — deploy runbook

**DEPLOYED 2026-10-28** (chain `junoclaw-g1`):
- **code_id 21** — tx `D631201D12B6B88BF38D831E595E3FB2EB295DA0A311F61F5846D8D48B7569E0` (h3,407,549)
- **contract `juno1rhlajgcuufadqwhjdw29fh4tged8cyxrtry4lt9luv54ex6emvdq44t7qm`** — tx `4CB4DD597B08607ED7ABAC8F45D8F3283ED96785C163CD89E53CF35112CDFCBF`
- admin = dao_core `juno122s3kcms43lgqu6p3h4tezcnwmpj5kslleku5jnz2a2jvat8mh0qkrqv3u` · cw4_group = `juno1u0l6ddahjn88v4usdrk33vk4k53qsjmmun45n9ssvfl5sg5xnj3qul37pj`
- verified: `get_config` + `list_active_seats` (empty, expected) on committed state
- NOTE: `TX_SENDER_CHAIN_ID=junoclaw-g1` required — default `junoclaw-1` → InvalidSignature at check_tx.

**Artifact:** `dist/junoclaw-seat-attestation-v0.1.0.wasm`
**sha256:** `6244e778f8dc10a3420f92cc678b8e325015ce59def6e328599ab73c5d00cd29`
(321,732 bytes — rebuild: `cargo +1.86.0 build --profile wasm --lib
--target wasm32-unknown-unknown -p junoclaw-seat-attestation`)

The contract is immutable-by-default here: instantiate with
`--admin <dao_core>` so the DAO can migrate at the bech32 cutover
(`UpdateConfig` also lets dao_core repoint cw4_group/dao_core in place).

## 0. Discover the addresses (ops host)

```bash
GROUP=$(sudo cat /opt/buzz/state/group_addr)
# dao_core via the proposal module's `dao` query:
DAO_CORE=$(Q=$(python3 -c 'import base64;print(base64.urlsafe_b64encode(b"{\"dao\":{}}").decode().rstrip("="))') \
  && curl -fsS "http://127.0.0.1:8082/api/wasm/smart/$(sudo cat /opt/buzz/state/proposal_addr)?query=$Q" \
  | python3 -c "import json,sys;print(json.load(sys.stdin)['data'])")
```

## 1. Store code (any funded key — code storage is permissionless)

```bash
tx-sender store-code --wasm dist/junoclaw-seat-attestation-v0.1.0.wasm --grpc 127.0.0.1:9090
# → note the code_id in the response
```

## 2. Instantiate with DAO admin

```bash
tx-sender instantiate --code-id <N> \
  --admin $DAO_CORE \
  --msg "{\"cw4_group_addr\":\"$GROUP\",\"dao_core_addr\":\"$DAO_CORE\"}" \
  --grpc 127.0.0.1:9090
# → contract addr via ContractsByCode query
```

## 3. Per-seat attestations (R3 ceremony step — after reshare)

Each seat's cold DAO key signs ONE `UpdateAttestation` tx — from the
operator's governance machine, never the validator host:

```json
{"update_attestation":{
  "seat_index": <0-9>,
  "hot_ed25519": "<base64 32B node p2p pubkey>",
  "bls_share_pubkey": "<base64 96B G2 share pubkey from keys.json>",
  "mayo_pubkey": "<base64 MAYO share pubkey or null>",
  "npub": "<base64 32B seat npub>"
}}
```

- seat_index is **first-claim**: the contract can't prove seat↔reshare
  roster; duplicates are rejected and disputes route through the DAO.
- Confirm with `{"list_active_seats":{}}` — every seat resolves to its
  member, npub, and keys.

## 4. Wire the consumers

- **claw-relay**: point `--member-bindings` at a file generated from
  `ListActiveSeats` (member→npub) — or later, query the contract
  directly (planned flag `--attestation-contract`; interim = cron that
  regenerates the bindings file from the contract).
- **Explorer/status**: per-seat identity (member + npub) instead of
  raw index.
- **Evidence**: `SubmitEvidence{seat_index,view,msg_a,sig_a,msg_b,sig_b}`
  — conflicting MinSig partials verified on-chain vs the bound G2
  share pubkey; a DAO proposal executes the penalty.

## Trust notes

- Cold key never touches a validator host — attestation is a one-time
  signed tx; rotation is another cold-signed tx.
- `PruneSeat` is permissionless: anyone can evict a record whose member
  lost cw4 weight. `ListActiveSeats` filters live membership each call,
  so the allowlist self-heals even before prune.
- Verified `submit_evidence` needs the real-BLS path (tested on unix;
  `MockApi` delegates to cosmwasm-crypto which wraps the same
  bls12_381 host functions the chain exposes).
