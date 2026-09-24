//! generate-testnet-keys: Offline BLS12-381 DKG key generation for a static 3-node testnet.
//!
//! This tool generates key material for a testnet with a configurable number of validators.
//! It uses `commonware-cryptography::bls12381::dkg::deal_anonymous` — a trusted dealing
//! function suitable for bootstrapping a testnet (not a production DKG ceremony).
//!
//! For each validator, the output JSON contains:
//! - `validator_index`: The validator's index in the participant set (0-based)
//! - `share_index`: The 1-based share index in the BLS polynomial
//! - `bls_private_hex`: Hex-encoded BLS secret share private scalar
//! - `bls_public_hex`: Hex-encoded BLS public share for this participant
//! - `threshold_public_key_hex`: Hex-encoded group threshold public key (same for all validators)
//! - `threshold_required`: Number of shares required to produce a threshold signature
//! - `threshold_total`: Total number of shares distributed
//! - `ed25519_private_hex`: Hex-encoded Ed25519 identity private key for P2P authentication
//! - `ed25519_public_hex`: Hex-encoded Ed25519 identity public key
//! - `validator_public_keys`: Ordered list of all validators' Ed25519 public keys (hex)
//!
//! Usage:
//!   cargo run -- --output-dir ./testnet-keys --validators 3
//!
//! Output files:
//!   ./testnet-keys/validator-0/keys.json
//!   ./testnet-keys/validator-1/keys.json
//!   ./testnet-keys/validator-2/keys.json
//!
//! NOTE: This uses trusted dealing — the tool knows all shares at generation time.
//! This is acceptable for testnet bootstrapping. For production, use an interactive
//! DKG ceremony with `commonware-cryptography::bls12381::dkg::{Dealer, Player}`.

use std::num::NonZeroU32;

use commonware_codec::codec::{Encode, Read};
use commonware_cryptography::{
    bls12381::{
        dkg::deal_anonymous,
        primitives::{
            group::Share,
            sharing::{Mode, ModeVersion, Sharing},
            variant::MinSig,
        },
    },
    ed25519, Signer, Verifier,
};
use commonware_math::algebra::Random;
use commonware_utils::N3f1;
use rand::rngs::OsRng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

/// Key material for a single validator node.
#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorKeys {
    /// 0-based validator index in the static participant set.
    pub validator_index: usize,

    /// 1-based share index in the BLS polynomial (from commonware Participant.get()).
    pub share_index: u32,

    /// Hex-encoded BLS secret share private scalar.
    pub bls_private_hex: String,

    /// Hex-encoded BLS public share for this participant (G2 point for MinSig).
    pub bls_public_hex: String,

    /// Hex-encoded group threshold public key (same for all validators).
    /// This is the key used to verify threshold signatures without knowing individual shares.
    pub threshold_public_key_hex: String,

    /// Minimum number of shares required to produce a valid threshold signature.
    pub threshold_required: u32,

    /// Total number of validators/shares in the set.
    pub threshold_total: u32,

    /// Hex-encoded Ed25519 identity private key for P2P peer authentication.
    pub ed25519_private_hex: String,

    /// Hex-encoded Ed25519 identity public key.
    pub ed25519_public_hex: String,

    /// Ordered list of all validators' Ed25519 public keys (hex-encoded).
    /// Position in this list is the validator's 0-based index.
    pub validator_public_keys: Vec<String>,

    /// Serialized BLS `Sharing` (mode + total + public polynomial).
    /// Populated by `finalize` after a Phase A ceremony; absent in legacy
    /// devnet key files (the node re-deals deterministically instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharing_hex: Option<String>,

    /// Serialized BLS `Share` (participant index + private scalar).
    /// Populated by `finalize`; absent in legacy devnet key files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_hex: Option<String>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Phase A ceremony subcommands. No subcommand → legacy devnet keygen.
    if let Some(cmd) = args.get(1).map(|s| s.as_str()) {
        match cmd {
            "keygen-share" => return cmd_keygen_share(&args[2..]),
            "assemble-genesis" => return cmd_assemble_genesis(&args[2..]),
            "finalize" => return cmd_finalize(&args[2..]),
            _ => {}
        }
    }

    legacy_main(&args);
}

/// Legacy centralized devnet keygen (trusted dealing, seeded RNG).
fn legacy_main(args: &[String]) {
    // Parse --output-dir and --validators flags
    let mut output_dir = "./testnet-keys".to_string();
    let mut num_validators: u32 = 3;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output-dir" => {
                i += 1;
                if i < args.len() {
                    output_dir = args[i].clone();
                }
            }
            "--validators" => {
                i += 1;
                if i < args.len() {
                    num_validators = args[i]
                        .parse()
                        .expect("--validators must be a positive integer");
                }
            }
            "--help" | "-h" => {
                println!("Usage: generate-testnet-keys [--output-dir <dir>] [--validators <n>]");
                println!("  --output-dir  Output directory (default: ./testnet-keys)");
                println!("  --validators  Number of validators (default: 3)");
                return;
            }
            _ => {}
        }
        i += 1;
    }

    if num_validators < 1 {
        eprintln!("Error: --validators must be >= 1");
        std::process::exit(1);
    }

    println!(
        "Generating BLS12-381 DKG key material for {num_validators}-node testnet..."
    );
    println!("Output directory: {output_dir}");

    // Use a deterministic RNG seeded with zero for reproducible testnet keys.
    // IMPORTANT: For any production use, use OsRng instead of a seeded RNG.
    // This tool is intentionally deterministic for testnet bootstrapping.
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    // Generate Ed25519 identity keys for all validators.
    let mut ed25519_keys: Vec<ed25519::PrivateKey> = Vec::new();
    for _ in 0..num_validators {
        ed25519_keys.push(ed25519::PrivateKey::random(&mut rng));
    }

    // Collect ordered list of all Ed25519 public keys (hex-encoded).
    let validator_public_keys: Vec<String> = ed25519_keys
        .iter()
        .map(|k| {
            use commonware_cryptography::Signer;
            hex::encode(k.public_key().encode().as_ref())
        })
        .collect();

    // Run offline BLS12-381 DKG using the trusted dealing function.
    //
    // `deal_anonymous` generates a random secret and distributes shares without
    // linking to specific participant identities. This is suitable for testnet
    // bootstrapping where we don't need to run the interactive P2P protocol.
    //
    // MinSig variant: public key in G2, signature in G1.
    // N3f1: BFT fault model requiring n >= 3f+1 participants.
    let n = NonZeroU32::new(num_validators).expect("num_validators > 0");
    let (sharing, shares) = deal_anonymous::<MinSig, N3f1>(&mut rng, Mode::NonZeroCounter, n);

    // Get the threshold public key (same for all validators).
    // For MinSig, the public key is a G2 point.
    let threshold_public_key = sharing.public();
    let threshold_public_key_hex = hex::encode(threshold_public_key.encode().as_ref());

    let threshold_required = sharing.required::<N3f1>();
    let threshold_total = sharing.total().get();

    println!(
        "BLS DKG complete: {threshold_required}/{threshold_total} threshold (N3f1)"
    );
    println!(
        "Group threshold public key: {}...",
        &threshold_public_key_hex[..std::cmp::min(32, threshold_public_key_hex.len())]
    );

    // Write key files for each validator.
    for (validator_index, (share, ed25519_key)) in
        shares.iter().zip(ed25519_keys.iter()).enumerate()
    {
        let dir = format!("{output_dir}/validator-{validator_index}");
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("Failed to create directory {dir}: {e}"));

        // Encode the BLS share private scalar using Encode trait.
        // Private.expose() allows us to access the inner scalar for encoding.
        let bls_private_hex = share.private.expose(|scalar| {
            hex::encode(scalar.encode().as_ref())
        });

        // Compute the BLS public share for this participant.
        // For MinSig, this is a G2 point.
        let bls_public_key = share.public::<MinSig>();
        let bls_public_hex = hex::encode(bls_public_key.encode().as_ref());

        // Encode the Ed25519 identity key pair.
        use commonware_cryptography::Signer;
        let ed25519_private_hex = hex::encode(ed25519_key.encode().as_ref());
        let ed25519_public = ed25519_key.public_key();
        let ed25519_public_hex = hex::encode(ed25519_public.encode().as_ref());

        let keys = ValidatorKeys {
            validator_index,
            share_index: share.index.get(),
            bls_private_hex,
            bls_public_hex,
            threshold_public_key_hex: threshold_public_key_hex.clone(),
            threshold_required,
            threshold_total,
            ed25519_private_hex,
            ed25519_public_hex,
            validator_public_keys: validator_public_keys.clone(),
            sharing_hex: None,
            share_hex: None,
        };

        let json = serde_json::to_string_pretty(&keys)
            .expect("JSON serialization of ValidatorKeys cannot fail");

        let path = format!("{dir}/keys.json");
        std::fs::write(&path, &json)
            .unwrap_or_else(|e| panic!("Failed to write {path}: {e}"));

        println!(
            "  validator-{validator_index}: {path} (share_index={}, ed25519_pub={}...)",
            share.index.get(),
            &keys.ed25519_public_hex[..std::cmp::min(16, keys.ed25519_public_hex.len())]
        );
    }

    println!("\nKey generation complete.");
    println!("Each validator-N/keys.json file contains:");
    println!("  - BLS secret share (bls_private_hex)");
    println!("  - BLS public share (bls_public_hex)");
    println!("  - Group threshold public key (threshold_public_key_hex)");
    println!("  - Ed25519 identity key pair (ed25519_private_hex, ed25519_public_hex)");
    println!("  - Ordered validator set (validator_public_keys)");
    println!("\nSet bls_key_path in your node config to point to the appropriate keys.json.");
    println!("WARNING: These keys were generated with a seeded RNG for reproducibility.");
    println!("         Do NOT use this tool for production deployments — use OsRng.");
}

// ──────────────────────────────────────────────
// Phase A ceremony tooling
// ──────────────────────────────────────────────
//
// Flow (see drafts/VALIDATOR_SET_DKG_PLAN.md §5):
//   1. Each validator runs `keygen-share` locally → publishes share-request.json
//      (Ed25519 pubkey + P2P address + self-attestation). Private key never leaves.
//   2. Coordinator collects all share-requests → runs `assemble-genesis` →
//      deals BLS shares with OsRng, emits per-validator bls-share.json
//      (distributed privately), shared.json, and node-<i>.toml templates.
//   3. Each validator runs `finalize` to merge their bls-share.json into
//      keys.json. The node loads sharing_hex/share_hex directly.
//
// Trust model: the coordinator transiently sees all BLS shares (trusted
// dealing). Ed25519 identity keys are self-generated and never shared.
// Phase C replaces dealing with the interactive Dealer/Player protocol.

/// Namespace for the Ed25519 attestation binding a share-request to its key.
const DKG_ATTEST_NAMESPACE: &[u8] = b"junoclaw-dkg-v1";

/// Public share-request published by each validator (output of keygen-share).
#[derive(Serialize, Deserialize, Debug)]
pub struct ShareRequest {
    /// Human-readable validator moniker (e.g. "ffern-1").
    pub moniker: String,
    /// P2P listen address peers will use to reach this validator (host:port).
    pub p2p_address: String,
    /// Hex-encoded Ed25519 identity public key.
    pub ed25519_public_hex: String,
    /// Hex-encoded Ed25519 signature over the pubkey bytes
    /// (namespace "junoclaw-dkg-v1"). Proves possession of the private key.
    pub attestation_hex: String,
}

/// Private BLS share package sent by the coordinator to one validator.
#[derive(Serialize, Deserialize, Debug)]
pub struct BlsSharePackage {
    /// 0-based validator index (sorted Ed25519 position).
    pub validator_index: usize,
    /// 1-based BLS share index.
    pub share_index: u32,
    /// Ed25519 pubkey this share is bound to (must match keys.json).
    pub ed25519_public_hex: String,
    /// Serialized `Sharing` (public polynomial — same for all validators).
    pub sharing_hex: String,
    /// Serialized `Share` (this validator's private share).
    pub share_hex: String,
    /// Hex-encoded group threshold public key.
    pub threshold_public_key_hex: String,
    /// Minimum shares required for a threshold signature.
    pub threshold_required: u32,
    /// Total shares in the set.
    pub threshold_total: u32,
    /// Ordered validator set (sorted Ed25519 pubkeys, hex).
    pub validator_public_keys: Vec<String>,
}

/// Public ceremony output — the assembled validator set (no secrets).
#[derive(Serialize, Deserialize, Debug)]
pub struct SharedCeremonyOutput {
    pub chain_id: String,
    pub threshold_public_key_hex: String,
    pub threshold_required: u32,
    pub threshold_total: u32,
    /// Ordered by sorted Ed25519 pubkey — position = validator_index.
    pub validators: Vec<SharedValidator>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SharedValidator {
    pub validator_index: usize,
    pub moniker: String,
    pub p2p_address: String,
    pub ed25519_public_hex: String,
}

fn get_arg<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// `keygen-share` — run by EACH validator on their own machine.
///
/// Generates the Ed25519 identity keypair with OsRng, writes a partial
/// keys.json (BLS fields empty until `finalize`), and a share-request.json
/// to publish to the coordinator.
fn cmd_keygen_share(args: &[String]) {
    let output_dir = get_arg(args, "--output-dir").unwrap_or("./my-validator").to_string();
    let moniker = get_arg(args, "--name").unwrap_or("validator").to_string();
    let p2p_address = get_arg(args, "--p2p").unwrap_or("0.0.0.0:7001").to_string();

    if has_flag(args, "--help") || has_flag(args, "-h") {
        println!("Usage: generate-testnet-keys keygen-share [--output-dir <dir>] [--name <moniker>] [--p2p <host:port>]");
        return;
    }

    let mut rng = OsRng;
    let ed_key = ed25519::PrivateKey::random(&mut rng);
    let ed_pub = ed_key.public_key();
    let ed_pub_bytes = ed_pub.encode();
    let ed25519_public_hex = hex::encode(ed_pub_bytes.as_ref());
    let ed25519_private_hex = hex::encode(ed_key.encode().as_ref());

    // Attestation: sign our own pubkey — proves possession of the private key.
    let attestation = ed_key.sign(DKG_ATTEST_NAMESPACE, ed_pub_bytes.as_ref());
    let attestation_hex = hex::encode(attestation.encode().as_ref());

    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|e| panic!("Failed to create {output_dir}: {e}"));

    // Partial keys.json — BLS fields filled by `finalize` after the ceremony.
    let keys = ValidatorKeys {
        validator_index: 0,
        share_index: 0,
        bls_private_hex: String::new(),
        bls_public_hex: String::new(),
        threshold_public_key_hex: String::new(),
        threshold_required: 0,
        threshold_total: 0,
        ed25519_private_hex,
        ed25519_public_hex: ed25519_public_hex.clone(),
        validator_public_keys: Vec::new(),
        sharing_hex: None,
        share_hex: None,
    };
    let keys_path = format!("{output_dir}/keys.json");
    std::fs::write(&keys_path, serde_json::to_string_pretty(&keys).unwrap())
        .unwrap_or_else(|e| panic!("Failed to write {keys_path}: {e}"));

    let request = ShareRequest {
        moniker: moniker.clone(),
        p2p_address: p2p_address.clone(),
        ed25519_public_hex: ed25519_public_hex.clone(),
        attestation_hex,
    };
    let req_path = format!("{output_dir}/share-request.json");
    std::fs::write(&req_path, serde_json::to_string_pretty(&request).unwrap())
        .unwrap_or_else(|e| panic!("Failed to write {req_path}: {e}"));

    println!("Validator identity generated (OsRng).");
    println!("  moniker:   {moniker}");
    println!("  ed25519:   {}...", &ed25519_public_hex[..16]);
    println!("  keys:      {keys_path}  (PRIVATE — never share)");
    println!("  request:   {req_path}  (publish to coordinator)");
    println!("\nNext: send share-request.json to the coordinator.");
    println!("After the ceremony, run `finalize` with the bls-share.json you receive.");
}

/// `assemble-genesis` — run by the COORDINATOR with all share-requests.
///
/// Verifies attestations, sorts validators by Ed25519 pubkey (deterministic
/// index assignment matching the node's sorted-position logic), deals BLS
/// shares with OsRng, and emits per-validator packages + shared.json +
/// node-<i>.toml templates.
fn cmd_assemble_genesis(args: &[String]) {
    let input_dir = get_arg(args, "--input-dir").unwrap_or("./share-requests").to_string();
    let output_dir = get_arg(args, "--output-dir").unwrap_or("./ceremony-out").to_string();
    let chain_id = get_arg(args, "--chain-id").unwrap_or("junoclaw-1").to_string();

    if has_flag(args, "--help") || has_flag(args, "-h") {
        println!("Usage: generate-testnet-keys assemble-genesis [--input-dir <dir>] [--output-dir <dir>] [--chain-id <id>]");
        return;
    }

    // Load every *.json in input-dir as a share-request.
    let mut requests: Vec<ShareRequest> = Vec::new();
    let entries = std::fs::read_dir(&input_dir)
        .unwrap_or_else(|e| panic!("Failed to read {input_dir}: {e}"));
    for entry in entries {
        let path = entry.expect("read_dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let contents = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
        let req: ShareRequest = serde_json::from_str(&contents)
            .unwrap_or_else(|e| panic!("{} is not a valid share-request: {e}", path.display()));
        requests.push(req);
    }

    if requests.len() < 4 {
        eprintln!("Error: need at least 4 share-requests for N3f1 consensus (got {})", requests.len());
        std::process::exit(1);
    }

    // Verify each attestation: signature over the pubkey under DKG namespace.
    for req in &requests {
        let pk_bytes = hex::decode(&req.ed25519_public_hex)
            .unwrap_or_else(|_| panic!("{}: bad ed25519_public_hex", req.moniker));
        let sig_bytes = hex::decode(&req.attestation_hex)
            .unwrap_or_else(|_| panic!("{}: bad attestation_hex", req.moniker));
        let pk = ed25519::PublicKey::read_cfg(&mut &pk_bytes[..], &())
            .unwrap_or_else(|_| panic!("{}: undecodable ed25519 pubkey", req.moniker));
        let sig = ed25519::Signature::read_cfg(&mut &sig_bytes[..], &())
            .unwrap_or_else(|_| panic!("{}: undecodable attestation", req.moniker));
        if !pk.verify(DKG_ATTEST_NAMESPACE, &pk_bytes, &sig) {
            panic!("{}: attestation verification failed — pubkey not signed by its owner", req.moniker);
        }
    }
    println!("Verified {} attestations.", requests.len());

    // Sort by Ed25519 pubkey bytes — deterministic validator_index, and it
    // matches the node's sorted-position share selection.
    requests.sort_by(|a, b| {
        let ab = hex::decode(&a.ed25519_public_hex).unwrap();
        let bb = hex::decode(&b.ed25519_public_hex).unwrap();
        ab.cmp(&bb)
    });

    // Duplicate pubkey check.
    for w in requests.windows(2) {
        if w[0].ed25519_public_hex == w[1].ed25519_public_hex {
            panic!("duplicate ed25519 pubkey: {} and {}", w[0].moniker, w[1].moniker);
        }
    }

    let validator_public_keys: Vec<String> =
        requests.iter().map(|r| r.ed25519_public_hex.clone()).collect();

    // Deal BLS shares with OsRng — real entropy, not the seeded devnet path.
    let n = NonZeroU32::new(requests.len() as u32).unwrap();
    let mut rng = OsRng;
    let (sharing, shares) = deal_anonymous::<MinSig, N3f1>(&mut rng, Mode::NonZeroCounter, n);

    let threshold_public_key_hex = hex::encode(sharing.public().encode().as_ref());
    let threshold_required = sharing.required::<N3f1>();
    let threshold_total = sharing.total().get();
    let sharing_hex = hex::encode(sharing.encode().as_ref());

    println!("BLS dealing complete: {threshold_required}/{threshold_total} threshold (N3f1, OsRng)");
    println!("Group threshold pubkey: {}...", &threshold_public_key_hex[..32]);

    std::fs::create_dir_all(&output_dir)
        .unwrap_or_else(|e| panic!("Failed to create {output_dir}: {e}"));

    // Per-validator private share package + node.toml template.
    for (i, (req, share)) in requests.iter().zip(shares.iter()).enumerate() {
        let dir = format!("{output_dir}/validator-{i}");
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|e| panic!("Failed to create {dir}: {e}"));

        let package = BlsSharePackage {
            validator_index: i,
            share_index: share.index.get(),
            ed25519_public_hex: req.ed25519_public_hex.clone(),
            sharing_hex: sharing_hex.clone(),
            share_hex: hex::encode(share.encode().as_ref()),
            threshold_public_key_hex: threshold_public_key_hex.clone(),
            threshold_required,
            threshold_total,
            validator_public_keys: validator_public_keys.clone(),
        };
        let pkg_path = format!("{dir}/bls-share.json");
        std::fs::write(&pkg_path, serde_json::to_string_pretty(&package).unwrap())
            .unwrap_or_else(|e| panic!("Failed to write {pkg_path}: {e}"));

        // node.toml template — peers = all OTHER validators.
        let mut toml = format!(
            "validator_index = {i}\n\
             chain_id = \"{chain_id}\"\n\
             p2p_listen = \"0.0.0.0:7001\"\n\
             grpc_listen = \"0.0.0.0:9090\"\n\
             bls_key_path = \"/keys/keys.json\"\n\
             identity_key_path = \"/keys/keys.json\"\n\
             data_dir = \"/data\"\n\
             genesis_path = \"\"\n\
             mempool_max_pending = 10000\n\
             leader_timeout_ms = 3000\n\
             certification_timeout_ms = 5000\n"
        );
        for (j, peer) in requests.iter().enumerate() {
            if j == i {
                continue;
            }
            toml.push_str(&format!(
                "\n[[peers]]\npublic_key = \"{}\"\naddress = \"{}\"\n",
                peer.ed25519_public_hex, peer.p2p_address
            ));
        }
        let toml_path = format!("{dir}/node-{i}.toml");
        std::fs::write(&toml_path, toml)
            .unwrap_or_else(|e| panic!("Failed to write {toml_path}: {e}"));

        println!(
            "  validator-{i}: {} ({}) — {pkg_path} [PRIVATE]",
            req.moniker, req.p2p_address
        );
    }

    // shared.json — public ceremony output.
    let shared = SharedCeremonyOutput {
        chain_id,
        threshold_public_key_hex,
        threshold_required,
        threshold_total,
        validators: requests
            .iter()
            .enumerate()
            .map(|(i, r)| SharedValidator {
                validator_index: i,
                moniker: r.moniker.clone(),
                p2p_address: r.p2p_address.clone(),
                ed25519_public_hex: r.ed25519_public_hex.clone(),
            })
            .collect(),
    };
    let shared_path = format!("{output_dir}/shared.json");
    std::fs::write(&shared_path, serde_json::to_string_pretty(&shared).unwrap())
        .unwrap_or_else(|e| panic!("Failed to write {shared_path}: {e}"));

    println!("\nCeremony assembly complete.");
    println!("  shared.json: {shared_path} (public)");
    println!("  validator-<i>/bls-share.json: send each to its owner over a secure channel");
    println!("  validator-<i>/node-<i>.toml: config template for each validator");
    println!("\nWARNING: bls-share.json files contain secret shares. Distribute");
    println!("         privately (encrypted DM, age, etc.) and delete local copies.");
}

/// `finalize` — run by EACH validator after receiving bls-share.json.
///
/// Merges the dealt share into their keys.json and verifies the share's
/// public key matches the sharing polynomial at its index.
fn cmd_finalize(args: &[String]) {
    let keys_path = get_arg(args, "--keys").unwrap_or("./keys.json").to_string();
    let share_path = get_arg(args, "--bls-share").unwrap_or("./bls-share.json").to_string();

    if has_flag(args, "--help") || has_flag(args, "-h") {
        println!("Usage: generate-testnet-keys finalize [--keys <keys.json>] [--bls-share <bls-share.json>]");
        return;
    }

    let mut keys: ValidatorKeys = serde_json::from_str(
        &std::fs::read_to_string(&keys_path)
            .unwrap_or_else(|e| panic!("Failed to read {keys_path}: {e}")),
    )
    .unwrap_or_else(|e| panic!("{keys_path} is not valid keys.json: {e}"));

    let package: BlsSharePackage = serde_json::from_str(
        &std::fs::read_to_string(&share_path)
            .unwrap_or_else(|e| panic!("Failed to read {share_path}: {e}")),
    )
    .unwrap_or_else(|e| panic!("{share_path} is not valid bls-share.json: {e}"));

    // The package must be addressed to OUR ed25519 key.
    if package.ed25519_public_hex != keys.ed25519_public_hex {
        eprintln!("Error: bls-share.json is bound to a different Ed25519 key.");
        eprintln!("  ours:    {}", keys.ed25519_public_hex);
        eprintln!("  package: {}", package.ed25519_public_hex);
        std::process::exit(1);
    }

    // Deserialize sharing + share; verify the share belongs to the polynomial.
    let sharing_bytes = hex::decode(&package.sharing_hex).expect("bad sharing_hex");
    let share_bytes = hex::decode(&package.share_hex).expect("bad share_hex");
    let n = NonZeroU32::new(package.threshold_total).expect("threshold_total > 0");
    let sharing = Sharing::<MinSig>::read_cfg(&mut &sharing_bytes[..], &(n, ModeVersion::v0()))
        .expect("undecodable sharing");
    let share = Share::read_cfg(&mut &share_bytes[..], &()).expect("undecodable share");

    let expected_pub = sharing
        .partial_public(share.index)
        .expect("share index out of range for polynomial");
    let actual_pub = share.public::<MinSig>();
    if expected_pub.encode().as_ref() != actual_pub.encode().as_ref() {
        eprintln!("Error: share does not match the sharing polynomial — package corrupt or mismatched.");
        std::process::exit(1);
    }

    // Merge into keys.json.
    keys.validator_index = package.validator_index;
    keys.share_index = package.share_index;
    keys.bls_private_hex = share
        .private
        .expose(|scalar| hex::encode(scalar.encode().as_ref()));
    keys.bls_public_hex = hex::encode(actual_pub.encode().as_ref());
    keys.threshold_public_key_hex = package.threshold_public_key_hex;
    keys.threshold_required = package.threshold_required;
    keys.threshold_total = package.threshold_total;
    keys.validator_public_keys = package.validator_public_keys;
    keys.sharing_hex = Some(package.sharing_hex);
    keys.share_hex = Some(package.share_hex);

    std::fs::write(&keys_path, serde_json::to_string_pretty(&keys).unwrap())
        .unwrap_or_else(|e| panic!("Failed to write {keys_path}: {e}"));

    println!("keys.json finalized.");
    println!("  validator_index: {}", keys.validator_index);
    println!("  share_index:     {}", keys.share_index);
    println!("  threshold:       {}/{}", keys.threshold_required, keys.threshold_total);
    println!("\nPoint bls_key_path + identity_key_path at {keys_path} in node.toml.");
    println!("Delete bls-share.json — its contents now live in keys.json.");
}
