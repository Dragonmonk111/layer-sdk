# PQ Phase 2 — Hybrid Protocol Authentication (Design)

Status: **design draft**. Phase 1 (hybrid tx auth: secp256k1 + MAYO) is implemented in
`packages/std/src/pubkey.rs` with a native verifier (`packages/junoclaw-mayo-verify`) and
an external signer (`tools/hybrid-sign`). This document extends the same hybrid
composition — classical AND post-quantum, both must verify — from transactions up into
validator identity and consensus certificates.

## 1. What exists today

| Layer | Algorithm | Where |
|---|---|---|
| P2P session auth | ed25519 | `app/slay3rd/src/main.rs` — `ed25519_{public,private}_hex` in key material; `LayerNode<_, ed25519::PublicKey>` |
| Consensus votes/certs | BLS12-381 threshold | `main.rs` — `bls12381_threshold::standard::Scheme`, DKG `deal_anonymous`, `threshold_required`/`threshold_total` |
| Block certificate | BLS threshold cert bytes | Reporter → `App::set_block_certificate` (`node.rs`) |
| Tx signatures | secp256k1, hybrid secp256k1+MAYO | `layer-std::pubkey` |
| Light client verification | BLS aggregate verify (on-chain) | `docs/BLS_LIGHT_CLIENT_SPEC.md`, `tools/bls-relayer` |

All consensus authentication is currently classical. A cryptographically relevant
quantum computer (or a break in BLS12-381 pairing assumptions) breaks validator
authentication entirely — forged certs, forged finality, forked light clients.

## 2. Design goals

- **Hybrid composition**: every consensus artifact requires BOTH a valid classical
  proof AND a valid PQ proof. Security = max(classical, PQ), not min.
- **Additive**: no change to vote timing, payload format, or simplex election logic.
  PQ material rides alongside existing fields.
- **Independent quorum**: PQ signatures cannot be aggregated via pairing, so the PQ
  half is a k-of-n bitmap cert (like a Tendermint commit), thresholded
  independently of the BLS threshold.
- **Light-client-safe**: the PQ cert must be verifiable by light clients; on-chain
  verification of MAYO in CosmWasm is a separate open question (§6).

## 3. Hybrid validator identity

Each validator's identity becomes a key bundle registered in genesis / key material:

```
ValidatorIdentity {
    ed25519_pk:  ed25519::PublicKey,      // P2P auth (unchanged)
    bls_share:   bls12381 share index,     // threshold share (unchanged, from DKG)
    mayo_pk:     HybridMayoPk,             // NEW — MAYO verification key
}
```

Encoding (mirrors the tx-layer hybrid pubkey in `layer-std::pubkey`):

```
hybrid_val_id_bytes = [mayo_variant: u8][ed_len: u8][ed25519 pk][mayo pk]
```

Key material file gains:

```json
"mayo_variant": "mayo2",
"mayo_private_hex": "…",   // MAYO signing key (sriracha-mayo keygen)
"mayo_public_hex": "…"
```

`participant_keys` in `main.rs` currently maps `ed25519::PublicKey` → simplex
participant index. Phase 2 extends the participant set to map `ed25519 pk →
mayo_pk` as well (in-memory table built from genesis identity bundles; DKG still
produces BLS shares only).

### Signing a consensus message

Every authenticated consensus message a validator emits (notarize/finalize vote,
nullify, proposal auth) carries a dual signature:

```
hybrid_sig = [ed25519_sig: 64B][mayo_sig: variable]
```

Both must verify against the registered `ValidatorIdentity` or the message is
rejected. Ed25519 covers classical security now; MAYO covers the quantum
adversary.

## 4. Hybrid consensus certificate

A finalized block's certificate becomes a two-part structure:

```
HybridCertificate {
    classical: bls12381_threshold cert,    // existing bytes, unchanged
    pq: PqCert,
}

PqCert {
    signer_bitmap: bitfield(n_validators), // which MAYO keys signed
    sigs:        [MayoSig; k],             // MAYO sigs, one per set bit, same digest
}
```

Validity rule (verified at cert time and by any consumer):

```
verify(HybridCertificate, digest) ≡
    bls_threshold_verify(classical, digest)          // ≥ threshold_required shares
AND count(valid_mayo_sigs over digest) ≥ pq_quorum    // independent quorum
```

`pq_quorum` is set equal to the BLS threshold (`threshold_required`) in Phase 2;
making it configurable is a config change in `key material`/`GenesisValidator`.

### Why not a PQ threshold scheme?

- BLS12-381 threshold exists today via `deal_anonymous` and is battle-tested in
  commonware; we keep it for the classical half.
- No deployed, audited PQ *threshold* signature exists that fits this slot
  (FROST is Schnorr-based = quantum-broken; lattice threshold schemes are
  research-stage). k-of-n MAYO is honest: it costs space but needs no new crypto.
- MAYO sigs are small enough that k ≤ ~7 sigs per cert is tolerable for a devnet
  validator set; the bitmap + fixed-size sig list keeps the cert constant-size
  for fixed n.

## 5. Integration points

| Change | File | Notes |
|---|---|---|
| Key material: add `mayo_{private,public}_hex`, `mayo_variant` | `main.rs` (`KeyMaterial`), gen tooling | Backward-compatible: absent = classical-only (devnet flag) |
| Hybrid vote sig on simplex messages | `main.rs` scheme wiring | Wrap `BlsScheme` signer: produce `hybrid_sig` over the same namespace'd bytes |
| Cert assembly: collect MAYO sigs per signer | `main.rs` Reporter / scheme | The scheme's `assemble` step picks k valid MAYO sigs matching quorum |
| Cert persist | `node.rs` — `set_block_certificate` | Bytes become `HybridCertificate` (version-tagged envelope) |
| Cert verify on ingest/backfill | `node.rs` | Backfilled blocks must carry both halves |
| Identity table | `main.rs` — participant_keys build | `ed25519 pk → mayo_pk` map for per-signer verify |
| Hybrid mode gate | `config.rs` | `--hybrid-consensus` flag / config; off = today (classical only) |

**Envelope versioning**: `certificate` bytes get a 1-byte tag —
`0x00` legacy BLS-only, `0x01` hybrid. Lets backfill/light clients distinguish.

## 6. Open questions

1. **On-chain PQ cert verify.** The BLS light client contract verifies the
   classical cert in CosmWasm. MAYO verify in WASM is possible (pure Rust in
   `junoclaw-mayo-verify`) but gas-unproven; alternatives:
   - relayer attests PQ half off-chain, contract checks only BLS (weaker),
   - native (non-contract) IBC verification path,
   - store PQ cert bytes on-chain and verify MAYO in a dedicated verification
     contract with a gas benchmark.
2. **Vote bandwidth.** Dual-signing every vote adds MAYO sig bytes per vote.
   MAYO2 signatures are small; acceptable for devnet, measure before mainnet
   validator sets grow.
3. **PQ share revocation/rotation.** MAYO key rotation needs an on-chain or
   config-registry path — reuse the validator-identity table.
4. **Which MAYO variant for consensus.** Consensus sigs are on the hot path;
   `mayo2` matches the tx default and keeps verify fast. Same variant registry
   as `layer-std::MayoVariant`.

## 7. Rollout

- **2a** (this doc → impl): hybrid identity in key material + `hybrid_sig` on
  votes behind `--hybrid-consensus`. Devnet only.
- **2b**: `HybridCertificate` assembly + versioned `certificate` bytes; backfill
  carries hybrid certs end-to-end.
- **2c**: light-client story for the PQ half (§6.1).
- **Gate**: enable hybrid only after a devnet run shows fork-free finality with
  all validators on hybrid sigs.

## 8. Status — shipped surfaces (2026-11-26)

Phase 2 is **live in production**, not a design target:

| Surface | Scheme | Status |
|---|---|---|
| Consensus votes + certs | BLS12-381 threshold + k-of-n MAYO2, AND'd | **Live** — `0x01` envelope on every finalized block (`app/slay3rd/src/hybrid_scheme.rs`) |
| Cert envelope versioning | 1-byte tag (`0x00` legacy / `0x01` hybrid) | Live — future schemes enter as `0x02` without wire changes |
| Account signatures | `PubKey::HybridSecp256k1Mayo` — spend requires BOTH halves | **Live on g1** — hybrid txs committed 2026-11-26; chain charges `GAS_COST_PQ_SIG_VALIDATION` |
| Hybrid address space | domain-separated `junoclaw-hybrid-v1` derivation | Live — cannot collide with plain secp256k1 accounts |
| State-sync anchors | verified against hybrid finality regardless of transport | Live |
| Signing tooling | `tools/hybrid-sign` — keygen / sign / raw-sign / mayo-sign / mayo-verify | Built (needs cmake + C toolchain — Docker/CI only) |
| Service signing | jc-faucet `--hybrid` env mode; bls-relayer `--jc-mayo-*` flags; clinical-evidence dual-signed bundles | In-tree, rolling out |
| PQ signature verify | `junoclaw-mayo-verify` (pure Rust, WASM-ready) | Live — charges its own gas line |
| Contract-layer sig verify | `mayo_verify` VM host function + `deps.api.mayo_verify()` | **Live in-tree** — contracts can require MAYO-2 alongside classical checks; classical-only contract sig checks are documented as quantum-forgeable |
| Ceremony file transit | jc-share `x25519-mlkem768-xchacha20poly1305-v2` — AEAD key from BOTH X25519 DH and ML-KEM-768 | In-tree — harvest-now resistant; `--kem-to` flag, `kem-keygen` derives KEM keypair from existing seed |
| Release authenticity | `junoclaw-release` MAYO key signs release manifests; pubkey anchored in-repo (`docs/RELEASE-KEY.json`) | Live — slay3rd-rc20 signed retroactively (`docs/release-sigs/`) |

### Surfaces that remain classical (documented, not hidden)

| Surface | Blast radius | Plan |
|---|---|---|
| ed25519 P2P transport auth | Session MITM/censorship — **cannot forge finalized state**: certs are hybrid-verified, state-sync anchors are finality-checked independent of transport | Track commonware upstream for a hybrid X25519+ML-KEM handshake; chain safety does not depend on it |
| TLS on edge endpoints (faucet/API/explorer) | Session interception of public data | Deploy hybrid TLS groups when infra supports; not chain-forging |
| Ceremony transit via v1 seals | Pre-v2 envelopes are classical-only | Operators adopt `--kem-to` v2 sealing; v1 decrypt path retained for archives |
| Plain secp256k1/ed25519 accounts | Still accepted by the app | Hybrid = documented default for new agent/service accounts; existing accounts migrate by rotation |

### Design property worth stating plainly

The composition is `max(classical, PQ)`, not `min`. If secp256k1 or BLS12-381
falls to any future break — quantum or classical — MAYO still holds every
finalized block and every hybrid account spend. If MAYO is found flawed, the
classical halves still hold. We are not betting on a scheme; we are betting on
a structure that survives scheme turnover, with a versioned envelope ready for
whatever NIST certifies next.
