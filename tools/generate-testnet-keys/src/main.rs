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

use commonware_codec::codec::Encode;
use commonware_cryptography::{
    bls12381::{
        dkg::deal_anonymous,
        primitives::{sharing::Mode, variant::MinSig},
    },
    ed25519,
};
use commonware_math::algebra::Random;
use commonware_utils::N3f1;
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
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

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
