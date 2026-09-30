# Key Management — Safety, Recoverability, and the Path to PQ

Status of every secret in the devnet, how each can be lost, how each is
recovered, and what changes before any state of value lives on this stack.

## 1. Key inventory

| Key | Material | Location | Purpose | Exposure |
|---|---|---|---|---|
| Validator BLS share + ed25519 identity (×4) | `bls_private_hex`, `ed25519_private_hex` in `keys.json` | `testnet-keys/validator-{0..3}/keys.json` — **tracked by git** | Simplex consensus votes; BLS threshold cert (3-of-4); group pubkey `91281b3f…` is the trust root of the 08-wasm light client | **Committed to a public GitHub branch** AND deterministically regenerable: `generate-testnet-keys` uses `ChaCha8Rng::seed_from_u64(0)` |
| Deployer / admin (JunoClaw) | secp256k1 privkey | Derived on the fly: `SHA256("junoclaw-deployer-v1")` → `juno1dz875zg8p78anpjv3f0qt4gu5a3awpjfhtw992` | Signs all `jc-*` txs (IBC handshake, store/instantiate/execute) | Seed string is public — anyone can rederive |
| Relayer (Osmosis) | secp256k1 privkey | `keyring-test` name `relayer` **inside the `local-osmosis` container** → `osmo14m9v8v33hdaydmz0cuszu339zgmpj5zeucn2w8` | Signs all counterparty txs (client updates, handshake, recv-packet) | Lives in a docker volume; exported to env at runtime by `ops/relayer/start-relay.ps1` |
| ~~Relayer (old)~~ | secp256k1 privkey | *lost* — was `osmo1aq995jf4fezcghl6ar6k79hk9layss8wyf6q0v` | Pre-re-genesis relayer signer | **Lost**: generated ad-hoc, hex never persisted outside an ephemeral shell. Lesson #1. |

## 2. What the 2026-09-28 wipe proved

- Cosmos-side chain state (Osmosis) survives `docker compose down -v` on the
  *junoclaw* compose project — the `.osmosisd` volume is separate. The relayer
  key in its keyring survived.
- JunoClaw validator keys survive because they live in the repo, not the volume.
- **Determinism cut both ways**: the root contract, deployer account, and
  validator set regenerated identically at genesis — good. But the same
  determinism means the committed `keys.json` files are not the secret; the
  *generator* is. Treat the repo as if the private keys were printed in it.
- The lost `osmo1aq995` key was unrecoverable because it had no backup and no
  deterministic path. Rule: **no key may exist whose only copy is in a shell
  variable or an unrecorded file.**

## 3. Rules (devnet, now)

- [ ] **Untrack `testnet-keys/` from git** (`git rm -r --cached testnet-keys`,
      add to `.gitignore`, keep local copies). Keys remain in history — rotation
      is the fix, not the removal.
- [ ] **Back up `testnet-keys/`** to `C:\cosmos-node\backups\testnet-keys-<date>.zip`
      alongside each devnet backup. Include `SHA256SUMS`.
- [ ] **Back up the osmosis keyring**: `docker exec local-osmosis osmosisd keys
      export relayer --keyring-backend test` → encrypted archive. The mnemonic
      (not just the raw hex) is the durable form.
- [ ] **Never pass private keys on a command line.** `start-relay.ps1` already
      uses env-var handoff; extend that pattern to any new script.
- [ ] **Record every funded address** in the ops record (`RECORD.md` style)
      *at funding time* — address, where the key lives, backup location.

## 4. Before testnet (hard requirements)

1. **Validator keys**: use the DKG path (`generate-testnet-keys --dkg` /
   `deal_anonymous` with `OsRng`, line ~497 of the tool) — no seeded RNG, no
   committed shares. Each operator holds their share; the group pubkey is the
   only public artifact.
2. **Deployer key**: replace `SHA256(seed)` derivation with a real key held in
   an OS keyring or HSM. Deterministic-from-public-string keys are devnet-only.
3. **Relayer key**: dedicated low-balance key per counterparty, keyring-backend
   `file`/`os` (not `test`), mnemonic backed up in encrypted cold storage.
   Relayer keys are hot — cap their balance to operational runway.
4. **Purge history** or rotate: anything ever committed (even devnet keys that
   later get funded with real faucet value) must be re-generated from `OsRng`.

## 5. PQ recoverability posture

- Consensus (BLS on BLS12-381, ed25519 identities) and account keys (secp256k1)
  are **all classically breakable** under CRQC assumptions. Nothing on the
  current chain is PQ-safe at rest.
- The mitigations already demonstrated: `jclaw-credential` verifies MAYO-2/3/5
  attestations on-chain (devnet records in `ICS20_LOCAL_OSMOSIS_DEMO.md`). That
  is the building block for PQ-authorized actions.
- Roadmap hooks:
  - **A56** (real Osmosis→JunoClaw verifier) and any future validator-auth path
    should define rotation to a PQ or hybrid signature at the *protocol* level.
  - Until then, the practical defense is **operational**: keys are cheap to
    rotate precisely because devnet keys are deterministic — testnet keys must
    keep that property via ceremony, not secrecy of a seed.

## 6. Cold backup — paper/metal and encrypted-at-rest

Files on disk alone are not a backup: one ransomware hit, one `rm`, one dead
NVMe and a hot key is gone. Every key that must survive this machine gets
two physical forms and one encrypted logical form.

### 6.1 What goes on paper/metal

- **Mnemonic, not hex.** Write down BIP-39 mnemonics where they exist
  (relayer key, testnet deployer). Where only raw hex exists (validator
  `keys.json`, devnet deployer `SHA256(seed)` derivation), record the
  *derivation input* (seed string, tool + flags) — that is the mnemonic
  equivalent, not the hex output.
- **Paper for devnet, metal for anything funded.** Stamped/punched steel
  survives the fire/flood cases paper doesn't. Testnet key ceremony output
  goes straight to metal; paper is an interim only.
- **Two copies, two locations.** A backup in the same building as the host
  protects against disk failure, not site loss. `C:\cosmos-node\backups\`
  currently fails this test — one copy must live off-host (encrypted zip on
  separate machine/USB; mnemonic in a different physical location).
- **No photos, no cloud notes, no password-manager free-text.** A mnemonic in
  plaintext on a synced service is not cold storage; it's a leak with extra
  steps. If a digital copy exists it must be the encrypted form below.

### 6.2 Encrypted-at-rest (the digital copy)

- `osmosisd keys export <name> --keyring-backend test` already emits an
  armored, passphrase-encrypted blob — that is the sanctioned encrypted form
  for account keys. Store the artifact, store the passphrase separately.
- For `keys.json` and other raw-hex material: `7z a -p -mhe=on backup.7z`
  (AES-256, header encrypted) or age/GPG equivalent. One passphrase for the
  archive; that passphrase goes on paper/metal, never in a file.
- Naming: `backups/keys-<YYYYMMDD>-<label>.7z`. Include a plaintext
  `SHA256SUMS` *outside* the archive so integrity can be checked without
  decrypting.

### 6.3 Rules of thumb

- If it isn't written down, it doesn't exist (the `osmo1aq995` lesson).
- If it's written down in exactly one place, it doesn't exist either.
- Cold backups are for secrets; checksums are public. Never mix them.
- Test restores: a backup that has never been restored is a rumor. Verify
  `--recover` from mnemonic before funding an address.

## 7. Recovery runbook

| Loss | Recovery |
|---|---|
| Devnet validator keys deleted | Re-run `tools/generate-testnet-keys` (seed 0) → identical `keys.json`; or restore `testnet-keys` backup zip |
| Relayer container/volume deleted | Import backed-up mnemonic → `osmosisd keys add relayer --recover --keyring-backend test`; re-fund |
| JunoClaw devnet wiped | Re-genesis reproduces root contract + validator set deterministically; rebuild IBC via `ibc-rebuild.ps1` (~1.5 min); re-deploy wasm contracts from `backups/junoclaw-state-inputs` (checksum-verified) |
| Osmosis state wiped | `.osmosisd` re-init loses relayer key + all 08-wasm clients; restore volume backup or re-init + re-import key + rerun full link script |
| Host lost entirely | Repo + `C:\cosmos-node\backups\` must both be off-host mirrored; today they are not — gap to close. Keys recoverable from §6.1 cold backups even if backups dir is lost |
