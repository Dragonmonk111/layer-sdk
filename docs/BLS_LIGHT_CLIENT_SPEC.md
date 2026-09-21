# JunoClaw BLS Light Client Specification (08-wasm)

**Status:** Draft v0.1 — pre-mainnet
**Depends on:** Phase A DKG ceremony (`VALIDATOR_SET_DKG_PLAN.md`), `slay3rd` consensus
**Target deployment:** CosmWasm contract implementing the IBC 08-wasm light client interface

---

## 1. Motivation

Light clients are the safest interoperability primitive: instead of trusting a
bridge's multisig or oracle set, the counterparty chain verifies **consensus
certificates produced by JunoClaw's own validator set**. A forged header requires
forging a BLS12-381 threshold signature — i.e. compromising `2f+1` validator
shares — not bribing a 5-of-9 bridge committee.

JunoClaw's consensus (commonware simplex, `bls12381_threshold::standard`,
`MinSig` variant) already produces a per-block finality certificate that is
**a single aggregated signature**. Verifying one block header costs **one
pairing check** — no per-validator signature iteration, no validator-set
diffs, no Tendermint-style commit verification over N signatures.

This makes JunoClaw headers cheap enough to verify inside a CosmWasm contract
on Juno (or any chain with a BLS12-381 host function), and — via the same
verification primitive — inside an EVM precompile call on Avalanche or a
native program on Solana.

## 2. Consensus Recap (what gets signed)

`slay3rd` runs commonware simplex with the `bls12381_threshold::standard`
scheme over the `MinSig` variant:

- **Signatures live in G1** (48 bytes compressed).
- **The group public key lives in G2** (96 bytes compressed).
- The validator set is **static** (Phase A DKG). The group public key is fixed
  at genesis and does not change per epoch — the light client stores it once.

When a quorum of validators finalize a proposal, the engine recovers a
threshold signature and emits a `Finalization`:

```text
Finalization {
    proposal: Proposal {
        round:   Round { epoch: u64, view: u64 },
        parent:  View  (u64),
        payload: sha256::Digest,        // 32-byte block payload commitment
    },
    certificate: G1,                    // recovered threshold signature (48 B)
}
```

**Verified against `tools/verify-cert`:** the persisted certificate is
**just the raw recovered G1 signature** — there is no signer bitmap. This is
the key property of a real threshold signature (vs. a multisig aggregate):
Lagrange interpolation over ≥ quorum shares produces a single point
indistinguishable from a signature by any other quorum subset. The DKG's
group public key already commits to the full validator set, so no
per-signer accounting is needed at verification time (it *is* still
recoverable off small-N committees for slashing — see §7 — but is not part
of the certificate itself).

The signed message is the canonical `commonware-codec` encoding of the
`Proposal` struct itself — **not** a further-wrapped `Subject::Finalize`
enum — under the namespace `union(b"slay3r-consensus-v1", b"_FINALIZE")`
(plain concatenation, no length prefix). The verification equation is
`ops::verify_message::<MinSig>(pubkey, namespace, proposal_bytes, signature)`,
which hashes `union_unique(namespace, proposal_bytes)` (LEB128-length-prefixed
namespace) to G1 under DST `BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_`.

The certificate is persisted per block height via
`App::set_block_certificate(height, cert_bytes)` (CONS-05). As of this spec,
the raw `proposal_bytes` are **also** persisted via the companion
`App::set_block_proposal(height, proposal_bytes)` (added alongside this spec,
see `packages/app/src/app.rs`) — required because the light client cannot
reconstruct `round`/`parent` from `height`/`timestamp`/`payload_digest` alone.
Both are available to relayers through the node API (coordination API wiring
for `proposal_bytes` is tracked in §11.5).

## 3. Security Model

| Property | Guarantee |
|---|---|
| Finality | A verified certificate means `≥ 2f+1` of the threshold group signed the proposal. Simplex finalization is irreversible — no fork can produce a conflicting finalized block at the same view without `≥ f+1` share compromise. |
| Trust assumption | The genesis group public key (anchored in client state). No per-block validator set trust, no relayer trust. |
| Equivocation | Two valid certificates for different payloads at the same `(epoch, view)` constitute cryptographic proof of `≥ f+1` share compromise — slashable evidence. |
| Liveness | Client only needs one certificate per height it cares about; no header chain required (certificates are self-contained). |

Because the validator set is static, there is **no validator-set-update path**
in v1 — the largest source of light client bugs (valset hash tracking,
trusting-period drift) is designed out entirely.

## 4. Client State

```rust
pub struct ClientState {
    /// Chain identifier, e.g. "junoclaw-1".
    pub chain_id: String,
    /// BLS12-381 G2 group public key from the Phase A ceremony (96 bytes, hex).
    pub group_public_key_hex: String,
    /// Latest verified height (IBC Height: revision 0, block height).
    pub latest_height: Height,
    /// Frozen on detected misbehaviour.
    pub frozen_height: Option<Height>,
}
```

`Height` is the IBC wire type `{ revision_number: u64, revision_height: u64 }`
— JunoClaw maps block height `h` to `(0, h)`. The Go-side
`WasmClientState.LatestHeight` must be kept in sync by the relayer when
constructing `MsgCreateClient`.

```rust
pub struct ConsensusState {
    /// sha256 payload digest from the finalized proposal (hex).
    pub payload_digest_hex: String,
    /// Block timestamp in nanoseconds (IBC convention) — carried by the relayer.
    pub timestamp: u64,
    /// Consensus epoch / view the block was finalized in.
    pub epoch: u64,
    pub view: u64,
    /// Parent view.
    pub parent: u64,
}
```

## 5. Header (what the relayer submits)

The header is JSON-encoded inside the 08-wasm `client_message` bytes
(base64 `[]byte` on the Go side):

```rust
pub struct Header {
    /// IBC height — { revision_number: 0, revision_height: block height }.
    pub height: Height,
    /// Block timestamp in nanoseconds (IBC convention).
    pub timestamp: u64,
    /// commonware-codec encoded Proposal { round, parent, payload } —
    /// exactly App::get_block_proposal(height). Base64.
    pub proposal_bytes: Binary,
    /// Raw 48-byte compressed G1 threshold signature —
    /// exactly App::get_block_certificate(height). Base64.
    pub certificate_bytes: Binary,
}
```

Both `proposal_bytes` and `certificate_bytes` are passed through verbatim —
the relayer does not re-encode or transform either field. Field names and
encodings match `ibc-go`'s `contract_api.go` payloads, which are marshaled
with `encoding/json` (`[]byte` → base64, proto structs → snake_case tags).

A misbehaviour report is `{ "header_a": Header, "header_b": Header }`
(same round or same height, conflicting payloads — see §7).

## 6. Verification Algorithm

```text
fn verify_header(client_state, header) -> Result<ConsensusState>:
    1. Reject if client_state.frozen_height.is_some().
    2. Reject if header.height <= client_state.latest_height
       (unless this is a misbehaviour check — see §7).
    3. Decode certificate_bytes as a compressed G1 point (48 bytes) — the
       recovered threshold signature.
    4. Decode proposal_bytes as Proposal { round: {epoch, view}, parent, payload }
       via the fixed field layout in §2 (round then parent then 32-byte digest).
    5. Pairing check (MinSig):
           namespace = concat(b"slay3r-consensus-v1", b"_FINALIZE")
           msg = varint(namespace.len()) || namespace || proposal_bytes
           hm  = hash_to_curve_G1(dst="BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_", msg)
           e(certificate ∈ G1, g2_generator) == e(hm, group_public_key ∈ G2)
       Reject if it fails.
    6. Return ConsensusState {
           payload_digest: proposal.payload,
           timestamp: header.timestamp,
           epoch: proposal.round.epoch,
           view:  proposal.round.view,
       }.
```

No signer-quorum check is needed at verification time (step removed from
earlier draft) — the recovered signature is only producible with ≥ quorum
shares in the first place; there is nothing to count.

**Gas cost:** one BLS12-381 pairing + one hash-to-curve. No native BLS12-381
host function exists in our cosmwasm-vm fork today (confirmed — only BN254 and
ML-DSA precompiles). Phase 1 therefore verifies **in-contract** using the
pure-Rust `bls12_381` (zkcrypto) crate, which is wasm32-unknown-unknown
compatible (no C dependency, unlike `blst`). This is gas-heavier than a host
precompile; adding a native `bls12_381_pairing` import is a v2 optimization.

## 7. Misbehaviour

```text
fn check_misbehaviour(client_state, h1, h2) -> Result<()>:
    1. Both headers must verify (run §6 steps 3–6 on each).
    2. If h1.height == h2.height AND h1.payload_digest != h2.payload_digest
       → freeze client at that height, emit evidence.
    3. Same for conflicting (epoch, view) with different payloads.
```

Two conflicting valid certificates are only possible if `≥ f+1` shares signed
both — the signer bitmaps in the two certificates identify the equivocating
share indices, which can be mapped back to Ed25519 operator identities for
off-chain accountability.

## 8. State Proofs (membership / non-membership)

The signed `BlockPayload` carries a `state_root` field — a Merkle root over
the application's post-state after the **previous** block (Tendermint
app-hash semantics: the proposer cannot know its own post-state, so block
H commits to the state root of H-1).

**Commitment layout** (implemented in `packages/app/src/app.rs`):

- Leaves: every committed KV entry whose raw key does **not** start with
  `'_'` (the sidecar convention — certificates, proposals, timestamps,
  payloads, and the root itself are excluded, matching `FastHasher`'s
  app_hash exclusion).
- `leaf = sha256(0x00 || key || value)`; `node = sha256(0x01 || l || r)`;
  odd nodes promote unchanged; the empty tree is `sha256("")`.
- The app recomputes the root at the end of every `finalize_block` (and at
  `init` for genesis) and stores it in the `_state_root` sidecar item;
  `propose()` reads it into `BlockPayload.state_root`.

**Proof wire format** — the `proof` field of `verify_membership` is opaque
to ibc-go; we carry JSON `MembershipProof`:

```json
{
  "payload_bytes": "<b64 bincode BlockPayload at proof height>",
  "leaf_index": 1,
  "siblings": ["<b64 32-byte sibling>", null, "..."]
}
```

`siblings[i]` is the sibling at tree level `i`, bottom-up; `null` marks a
promotion level (odd node count — the node moves up unchanged).

**Verification** (`contracts/light-client/src/contract.rs`):

```text
verify_membership(height, proof, merkle_path, value):
    1. consensus = consensus_states[height]            (else NotFound)
    2. require sha256(proof.payload_bytes)
              == consensus.payload_digest              (binds proof to
                                                        the certificate)
    3. payload = bincode::deserialize(payload_bytes)
       state_root = payload.state_root                 (post-state of
                                                        height-1)
    4. key = concat(merkle_path.key_path)              (ICS-24 path =
                                                        storage key)
    5. leaf = sha256(0x00 || key || value)
    6. walk siblings → root; require == state_root
```

**Height convention:** to prove state at height X the relayer submits
`proof_height = X + 1` — the consensus state at X+1 stores the payload
whose `state_root` covers post-state(X). Same off-by-one as Tendermint's
`app_hash`.

**Serving:** `layer.lightclient.v1.Query/Proof(key)` builds the path over
the latest committed state in one storage snapshot and returns
`{ state_height, key, value, leaf_index, siblings }`. The relayer pairs it
with `Block(state_height + 1)` to fetch `payload_bytes` and assembles the
contract-side proof. The node also persists every block's full payload
under `_payload/{height}` (written in `execute_block`) so `payload_bytes`
is always available for finalized heights.

**Non-membership** is not supported in v1 — proving a key's absence needs
a versioned/range-provable tree (JMT/IAVL-style), which is the documented
scaling path once state size or timeout flows require it. ICS-20
recv/ack flows only need membership; timeout flows need non-membership
and are deferred.

## 9. 08-wasm Integration

The contract implements the exact `ibc-go` `08-wasm` contract ABI (verified
against `modules/light-clients/08-wasm/types/contract_api.go` on ibc-go
main — API-identical to v8.3+/v9/v10/v11 for these payloads). Field names
match the Go `encoding/json` marshaling of the payload structs:

**`instantiate`** — payload `InstantiateMessage`:
`{ "client_state": <b64 JSON §4 ClientState>,
   "consensus_state": <b64 JSON §4 ConsensusState>,
   "checksum": <b64> }`. Validates the group public key (96 bytes) eagerly,
stores the initial consensus state at `latest_height`.

**`sudo`** — `SudoMsg` variants:

| Variant | Behaviour | Status |
|---|---|---|
| `update_state` | §6 + monotonicity + equivocation guard; stores consensus state, bumps `latest_height`, returns `{ "heights": [Height] }`. Idempotent on the same header; rejects a conflicting header at a stored height (route it to misbehaviour instead). | ✅ |
| `update_state_on_misbehaviour` | §7 — two valid certs that equivocate → sets `frozen_height` = min height, returns `{}`. | ✅ |
| `verify_membership` | §8 — digest binding + bincode `state_root` extraction + Merkle path walk; returns `{}` on success. | ✅ |
| `verify_non_membership` | §8 — explicit error; needs a versioned tree (deferred). | ⏳ |
| `verify_upgrade_and_update_state` | Not supported in v1 (returns error). | ❌ |
| `migrate_client_store` | No-op `{}` (v1 layouts are stable). | ✅ |

**`query`** — `QueryMsg` variants:

| Variant | Response | Status |
|---|---|---|
| `status` | `{ "status": "Active" \| "Frozen" }` | ✅ |
| `timestamp_at_height` | `{ "timestamp": <nanos> }` from the stored consensus state | ✅ |
| `verify_client_message` | Dry-runs §6 (header) or §7 (misbehaviour) without mutating state; errors on invalid input | ✅ |
| `check_for_misbehaviour` | `{ "found_misbehaviour": bool }` — parses as a misbehaviour report, verifies, never errors | ✅ |

**Toolchain note:** the contract builds for `wasm32-unknown-unknown` via
`cargo build -p junoclaw-light-client --release --target
wasm32-unknown-unknown`. This required a one-line fix in the vendored
cosmwasm fork (`lib/cosmwasm/packages/std/src/imports.rs`): modern rust-lld
no longer maps bare `extern "C"` blocks to implicit `env`-module imports,
which broke **every** contract build (`undefined symbol: db_read`, …). The
extern block now carries `#[cfg_attr(target_arch = "wasm32",
link(wasm_import_module = "env"))]`, matching the VM's
`register_namespace("env", …)` and the fix other ecosystems adopted for
the same toolchain change (e.g. near/near-sdk-rs#1550).

On any chain running 08-wasm, the contract is instantiated per client via
`MsgCreateClient`; the relayer constructs the outer `WasmClientState` proto
with `Data` = the §4 client-state JSON and `LatestHeight` in sync. The same
wasm blob serves all counterparties.

## 10. Beyond IBC — Avalanche, Solana, EVM

The verification primitive (§6) is portable because it is **one pairing**:

- **EVM chains (Avalanche C-Chain, Ethereum L2s):** verify via the
  EIP-2537 BLS12-381 precompile (`0x0f` pairing). A Solidity adapter contract
  decodes the header, builds the pairing input, and stores verified payload
  digests — same trust model, no IBC stack required.
- **Solana:** a native program performs the pairing with
  `solana_bls12_381` syscalls (or the alt_bn128-style syscall once BLS12-381
  lands); verified payload digests gate a message mailbox account.
- **Off-chain / agents:** any Rust or TS service can verify certificates
  directly against the group pubkey — useful for J-Lens audit trails and
  robot attestation consumers that don't need on-chain verification.

In every deployment the trust root is identical: the Phase A group public key.

## 11. Open Questions

1. ~~**Batch commitment layout**~~ — **resolved:** `BlockPayload.state_root`
   commits to a domain-separated Merkle root over all non-`'_'` KV entries
   of the previous block's post-state (§8). `payload_digest` remains a flat
   `sha256(bincode(payload))`; the state root rides inside it.
2. **Equivocation → operator mapping** — since certificates carry no signer
   bitmap, identifying which validators contributed to a forged quorum after
   the fact for slashing requires falling back to the DKG dealer logs /
   individual partial-signature messages exchanged during consensus, not the
   certificate itself. Needs a design pass if on-chain slashing evidence is
   required (§7 currently only proves *that* equivocation occurred, not *who*).
3. **Timestamp authority** — header timestamps come from the batch, not the
   certificate. For packet timeouts we may want the timestamp inside the
   signed payload (requires a consensus-level change; defer to v2).
4. **Epoch transitions** — Phase B (dynamic validator set / re-sharing) will
   need a `NextGroupPublicKey` commitment signed by the outgoing group.
   Designed out of v1 deliberately.
5. ~~**Coordination API wiring**~~ — **resolved:** the node now serves
   `layer.lightclient.v1.Query` over gRPC (`LatestHeight`, `Block`,
   `Proof`) — proposal bytes, certificate, timestamp, and Merkle
   membership proofs for every finalized height.

## 12. Implementation Plan

| Phase | Deliverable |
|---|---|
| 0 | This spec + certificate format freeze (codec version `ModeVersion::v0`). |
| 1 | `junoclaw-light-client` CosmWasm contract: §6 verification via the pure-Rust `bls12_381` crate (in-contract, no host precompile); unit tests with real ceremony-dry-run certificates via `tools/verify-cert`-equivalent fixtures. |
| 2 | Relayer path: fetch `cert_bytes` by height → submit `Header` → `UpdateState`. |
| 3 | 08-wasm deployment on uni-7 (Juno testnet) + first verified header. |
| 4 | ~~Membership proofs~~ **done** — `state_root` in `BlockPayload`, `verify_membership` implemented, `Proof` RPC live. Next: ICS-20-style transfer demo. |
| 5 | EVM adapter (EIP-2537) for Avalanche C-Chain. |
