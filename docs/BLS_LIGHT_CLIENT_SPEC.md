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
    /// BLS12-381 G2 group public key from the Phase A ceremony (96 bytes).
    pub group_public_key: Vec<u8>,
    /// Total shares n and quorum threshold (N3f1: n = 3f+1, quorum = 2f+1).
    pub share_count: u32,
    pub quorum: u32,
    /// Latest verified height.
    pub latest_height: u64,
    /// Frozen on detected misbehaviour.
    pub frozen_height: Option<u64>,
}
```

```rust
pub struct ConsensusState {
    /// sha256 payload digest from the finalized proposal.
    pub payload_digest: [u8; 32],
    /// Block timestamp (from the batch, for packet timeouts).
    pub timestamp: u64,
    /// Consensus round the block was finalized in.
    pub epoch: u64,
    pub view: u64,
}
```

## 5. Header (what the relayer submits)

```rust
pub struct Header {
    /// Block height on JunoClaw.
    pub height: u64,
    /// commonware-codec encoded Proposal { round, parent, payload } —
    /// exactly App::get_block_proposal(height).
    pub proposal_bytes: Vec<u8>,
    /// Raw 48-byte compressed G1 threshold signature —
    /// exactly App::get_block_certificate(height).
    pub certificate_bytes: Vec<u8>,
    /// Block timestamp carried alongside (from StoredBlock).
    pub timestamp: u64,
}
```

Both `proposal_bytes` and `certificate_bytes` are passed through verbatim —
the relayer does not re-encode or transform either field.

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

The `payload_digest` commits to the block's batch. To prove a specific
message, packet commitment, or acknowledgement:

```text
verify_membership(consensus_state, proof, path, value):
    1. proof = Merkle path from the leaf to the batch root.
    2. Recompute root; require == consensus_state.payload_digest
       (or a sub-commitment within the batch — see Open Questions §11).
```

v1 assumes the batch hash is a Merkle root over `messages[]` /
`breaker_actions[]` — the exact commitment layout is pinned down when the
first proof consumer (ICS-20-style transfer or message-passing) is built.

## 9. 08-wasm Integration

The contract implements the standard 08-wasm light client entrypoints so
`ibc-go`'s wasm module can drive it:

| Entrypoint | Behaviour |
|---|---|
| `Initialize` | Store `ClientState` (group pubkey, chain-id, quorum params). |
| `VerifyClientMessage` | Run §6 on a submitted `Header`. |
| `CheckForMisbehaviour` | Run §7. |
| `UpdateState` | Store new `ConsensusState` at `header.height`; bump `latest_height`. |
| `VerifyMembership` / `VerifyNonMembership` | Run §8. |
| `Status` | `Active` unless frozen or expired. |
| `ExportMetadata` / `TimestampAtHeight` | Standard. |

On Juno, the contract is uploaded once and instantiated per counterparty
connection. The same wasm blob can serve any chain running 08-wasm.

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

1. **Batch commitment layout** — is `payload_digest` a Merkle root over
   messages, or a flat hash? Membership proofs need the former; if the batch
   hash is flat today, we add a `messages_root` field to the block record.
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
5. **Coordination API wiring** — `App::get_block_proposal` exists node-side
   (packages/app/src/app.rs) but is not yet exposed through the
   `junoclaw-coordination` REST API consumed by relayers/miners. Needs a
   `proposal_hex` field added to `StoredBlock` (mirrors `certificate`).

## 12. Implementation Plan

| Phase | Deliverable |
|---|---|
| 0 | This spec + certificate format freeze (codec version `ModeVersion::v0`). |
| 1 | `junoclaw-light-client` CosmWasm contract: §6 verification via the pure-Rust `bls12_381` crate (in-contract, no host precompile); unit tests with real ceremony-dry-run certificates via `tools/verify-cert`-equivalent fixtures. |
| 2 | Relayer path: fetch `cert_bytes` by height → submit `Header` → `UpdateState`. |
| 3 | 08-wasm deployment on uni-7 (Juno testnet) + first verified header. |
| 4 | Membership proofs + ICS-20-style transfer demo. |
| 5 | EVM adapter (EIP-2537) for Avalanche C-Chain. |
