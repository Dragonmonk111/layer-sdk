//! verify-cert: Offline BLS12-381 threshold signature certificate verifier.
//!
//! This tool verifies BLS12-381 threshold signature certificates produced by
//! the slay3rd consensus node (commonware threshold_simplex). It is designed
//! for offline verification of block finalization certificates stored in
//! `Block.certificate` (CONS-05).
//!
//! # Certificate format
//!
//! The certificate is the encoded bytes of a recovered BLS12-381 threshold
//! signature. For the `MinSig` variant:
//! - Signature is a G1 point (48 bytes compressed)
//! - Threshold public key is a G2 point (96 bytes compressed)
//!
//! # Signature context
//!
//! The finalization certificate is signed over:
//! - Subject: `Subject::Finalize { proposal }`
//! - Namespace: `union(base_namespace, b"_FINALIZE")`
//!   where `base_namespace = b"slay3r-consensus-v1"`
//! - Message: encoded proposal bytes (consensus Proposal struct)
//!
//! # Usage
//!
//! Verification using certificate bytes and threshold public key:
//!
//!   cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
//!     --cert <cert_hex> \
//!     --pubkey <threshold_pubkey_hex> \
//!     --message <message_hex> \
//!     [--namespace <namespace_hex>]
//!
//! Presence check only (no cryptographic verification):
//!
//!   cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
//!     --cert-file /tmp/layer-testnet/latest_cert.hex \
//!     --check-presence
//!
//! Read certificate from a JSON key file (like those from generate-testnet-keys):
//!
//!   cargo run --manifest-path tools/verify-cert/Cargo.toml -- \
//!     --cert <cert_hex> \
//!     --keys-file /tmp/layer-testnet/validator-0/keys.json \
//!     --message <message_hex>

use std::process;

use commonware_codec::extensions::DecodeExt as _;
use commonware_cryptography::bls12381::primitives::{
    group::{G1, G2},
    ops,
    variant::MinSig,
};

/// Key material file format (from generate-testnet-keys).
#[derive(serde::Deserialize, Debug)]
struct KeysFile {
    threshold_public_key_hex: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        process::exit(0);
    }

    // Parse arguments
    let mut cert_hex: Option<String> = None;
    let mut cert_file: Option<String> = None;
    let mut pubkey_hex: Option<String> = None;
    let mut keys_file: Option<String> = None;
    let mut message_hex: Option<String> = None;
    let mut namespace_hex: Option<String> = None;
    let mut check_presence = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--cert" => {
                i += 1;
                if i < args.len() {
                    cert_hex = Some(args[i].clone());
                }
            }
            "--cert-file" => {
                i += 1;
                if i < args.len() {
                    cert_file = Some(args[i].clone());
                }
            }
            "--pubkey" => {
                i += 1;
                if i < args.len() {
                    pubkey_hex = Some(args[i].clone());
                }
            }
            "--keys-file" => {
                i += 1;
                if i < args.len() {
                    keys_file = Some(args[i].clone());
                }
            }
            "--message" => {
                i += 1;
                if i < args.len() {
                    message_hex = Some(args[i].clone());
                }
            }
            "--namespace" => {
                i += 1;
                if i < args.len() {
                    namespace_hex = Some(args[i].clone());
                }
            }
            "--check-presence" => {
                check_presence = true;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    // Load certificate bytes
    let cert_bytes = if let Some(hex) = cert_hex {
        match hex::decode(hex.trim()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode --cert hex: {e}");
                process::exit(2);
            }
        }
    } else if let Some(path) = cert_file {
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("INVALID: Failed to read cert file '{path}': {e}");
                process::exit(2);
            }
        };
        match hex::decode(contents.trim()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode hex from cert file '{path}': {e}");
                process::exit(2);
            }
        }
    } else {
        eprintln!("INVALID: Provide --cert <hex> or --cert-file <path>");
        print_usage();
        process::exit(1);
    };

    // Presence check: just verify the certificate is non-empty
    if check_presence {
        if cert_bytes.is_empty() {
            eprintln!("INVALID: Certificate is empty (zero bytes) — Block.certificate is None or empty");
            process::exit(3);
        }
        println!(
            "PRESENT: Certificate has {} bytes — Block.certificate is Some (not None) (CONS-05)",
            cert_bytes.len()
        );
        println!("  Certificate hex (first 16 bytes): {}...", hex::encode(&cert_bytes[..cert_bytes.len().min(16)]));
        println!("");
        println!("To perform cryptographic verification, provide --pubkey and --message.");
        process::exit(0);
    }

    // Full cryptographic verification requires pubkey and message.
    if message_hex.is_none() {
        eprintln!("INVALID: --message <hex> is required for cryptographic verification");
        eprintln!("  (Use --check-presence to only verify the certificate is non-empty)");
        process::exit(1);
    }

    // Load threshold public key
    let pubkey_bytes = if let Some(hex) = pubkey_hex {
        match hex::decode(hex.trim()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode --pubkey hex: {e}");
                process::exit(2);
            }
        }
    } else if let Some(path) = keys_file {
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("INVALID: Failed to read keys file '{path}': {e}");
                process::exit(2);
            }
        };
        let keys: KeysFile = match serde_json::from_str(&contents) {
            Ok(k) => k,
            Err(e) => {
                eprintln!("INVALID: Failed to parse keys file '{path}': {e}");
                process::exit(2);
            }
        };
        match hex::decode(&keys.threshold_public_key_hex) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode threshold_public_key_hex from '{path}': {e}");
                process::exit(2);
            }
        }
    } else {
        eprintln!("INVALID: Provide --pubkey <hex> or --keys-file <path>");
        print_usage();
        process::exit(1);
    };

    // Decode message bytes
    let message_bytes = match hex::decode(message_hex.unwrap().trim()) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("INVALID: Failed to decode --message hex: {e}");
            process::exit(2);
        }
    };

    // Derive namespace
    // Default: finalize namespace = union(b"slay3r-consensus-v1", b"_FINALIZE")
    // This matches the Namespace::finalize field computed in slay3rd.
    let namespace_bytes: Vec<u8> = if let Some(ns_hex) = namespace_hex {
        match hex::decode(ns_hex.trim()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode --namespace hex: {e}");
                process::exit(2);
            }
        }
    } else {
        // Default: slay3r-consensus-v1_FINALIZE
        // commonware_utils::union(a, b) = concat(a, b) (no length prefix)
        let mut ns = b"slay3r-consensus-v1".to_vec();
        ns.extend_from_slice(b"_FINALIZE");
        ns
    };

    // Print inputs for audit trail
    println!("=== BLS12-381 Certificate Verification (CONS-05) ===");
    println!("");
    println!("  Certificate:    {} bytes", cert_bytes.len());
    println!("  Public key:     {} bytes (G2 compressed)", pubkey_bytes.len());
    println!("  Message:        {} bytes", message_bytes.len());
    println!("  Namespace:      {}", std::str::from_utf8(&namespace_bytes).unwrap_or("<binary>"));
    println!("");

    // Decode BLS12-381 components
    // For MinSig: Public key is G2 (96 bytes), Signature is G1 (48 bytes)

    // Decode threshold public key (G2)
    let threshold_pubkey = match G2::decode(bytes::Bytes::from(pubkey_bytes.clone())) {
        Ok(pk) => pk,
        Err(e) => {
            eprintln!("INVALID: Failed to decode threshold public key as G2 point: {e}");
            eprintln!("  Expected 96-byte compressed G2 point (MinSig variant)");
            eprintln!("  Got {} bytes: {}", pubkey_bytes.len(), hex::encode(&pubkey_bytes));
            process::exit(3);
        }
    };

    // Decode certificate as G1 signature
    // The certificate from LayerReporter is finalization.certificate.encode().to_vec()
    // Certificate<MinSig> wraps a Lazy<G1> (48-byte compressed G1 point)
    let signature = match G1::decode(bytes::Bytes::from(cert_bytes.clone())) {
        Ok(sig) => sig,
        Err(e) => {
            eprintln!("INVALID: Failed to decode certificate as G1 signature: {e}");
            eprintln!("  Expected 48-byte compressed G1 point (MinSig signature variant)");
            eprintln!("  Got {} bytes: {}", cert_bytes.len(), hex::encode(&cert_bytes));
            process::exit(3);
        }
    };

    // Verify the BLS12-381 threshold signature
    // ops::verify_message::<MinSig>(pubkey, namespace, message, signature)
    // This verifies: e(signature, G2::generator) == e(H(namespace || message), pubkey)
    // where H is the BLS hash-to-curve function with DST = MinSig::MESSAGE
    match ops::verify_message::<MinSig>(&threshold_pubkey, &namespace_bytes, &message_bytes, &signature) {
        Ok(()) => {
            println!("VALID: BLS12-381 threshold certificate verified successfully");
            println!("");
            println!("  Threshold public key: {}...", hex::encode(&pubkey_bytes[..pubkey_bytes.len().min(16)]));
            println!("  Signature (cert):     {}...", hex::encode(&cert_bytes[..cert_bytes.len().min(16)]));
            println!("  Message digest:       {}", hex::encode(&message_bytes));
            println!("  Namespace:            {:?}", std::str::from_utf8(&namespace_bytes).unwrap_or("<binary>"));
            println!("");
            println!("  Block.certificate is Some (non-None, non-empty) — CONS-05 SATISFIED");
            process::exit(0);
        }
        Err(e) => {
            eprintln!("INVALID: Certificate verification failed: {e:?}");
            eprintln!("");
            eprintln!("  Possible causes:");
            eprintln!("  1. Certificate bytes are not from this block (payload digest mismatch)");
            eprintln!("  2. Threshold public key does not match the signing committee");
            eprintln!("  3. Namespace mismatch (use --namespace to override)");
            eprintln!("  4. Message bytes are not the encoded Proposal (need encoded consensus Proposal)");
            eprintln!("");
            eprintln!("  For presence verification only (non-cryptographic), use --check-presence");
            process::exit(4);
        }
    }
}

fn print_usage() {
    println!("verify-cert: Offline BLS12-381 threshold signature certificate verifier (CONS-05)");
    println!("");
    println!("USAGE:");
    println!("  verify-cert --cert <hex> --pubkey <hex> --message <hex> [--namespace <hex>]");
    println!("  verify-cert --cert <hex> --keys-file <path> --message <hex>");
    println!("  verify-cert --cert-file <path> --check-presence");
    println!("");
    println!("OPTIONS:");
    println!("  --cert <hex>           Certificate bytes as hex (G1 signature, 48 bytes for MinSig)");
    println!("  --cert-file <path>     Path to file containing certificate hex");
    println!("  --pubkey <hex>         Threshold public key as hex (G2 point, 96 bytes for MinSig)");
    println!("  --keys-file <path>     Path to validator keys.json (from generate-testnet-keys)");
    println!("                         Uses threshold_public_key_hex field");
    println!("  --message <hex>        Signed message as hex (encoded consensus Proposal bytes)");
    println!("  --namespace <hex>      Namespace bytes as hex (default: slay3r-consensus-v1_FINALIZE)");
    println!("  --check-presence       Only verify cert is non-empty, skip cryptographic check");
    println!("  --help, -h             Print this help");
    println!("");
    println!("EXIT CODES:");
    println!("  0  Certificate is VALID (or PRESENT with --check-presence)");
    println!("  1  Usage error (bad arguments)");
    println!("  2  Input decode error (bad hex, bad JSON)");
    println!("  3  Decode error (bad BLS point encoding)");
    println!("  4  Certificate verification FAILED");
    println!("");
    println!("EXAMPLES:");
    println!("  # Cryptographic verification:");
    println!("  verify-cert --cert 8abc... --pubkey 9def... --message 1234...");
    println!("");
    println!("  # Using keys file (from generate-testnet-keys):");
    println!("  verify-cert --cert 8abc... --keys-file /tmp/layer-testnet/validator-0/keys.json --message 1234...");
    println!("");
    println!("  # Presence check only (verify Block.certificate is Some/non-empty):");
    println!("  verify-cert --cert-file /tmp/layer-testnet/latest_cert.hex --check-presence");
}
