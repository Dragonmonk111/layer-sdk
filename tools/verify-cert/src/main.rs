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
use junoclaw_mayo_verify::{ParameterSet as _, Mayo2};

/// Key material file format (from generate-testnet-keys).
#[derive(serde::Deserialize, Debug)]
struct KeysFile {
    threshold_public_key_hex: String,
    #[serde(default)]
    threshold_required: Option<usize>,
    #[serde(default)]
    validator_public_keys: Option<Vec<String>>,
    #[serde(default)]
    validator_mayo_public_keys: Option<Vec<String>>,
}

/// Hybrid certificate envelope (PQ_PROTOCOL_AUTH.md §5):
/// `[0x01][classical BLS cert: 48 B][signers BitMap][MAYO2 sig: 186 B × k]`
/// where the BitMap is `[u64 BE: bit count][ceil(count/8) bytes]`
/// (commonware_utils::bitmap::BitMap<1> codec) and `k` = popcount.
struct HybridEnvelope {
    classical: Vec<u8>,
    participants: usize,
    signers: Vec<usize>,
    mayo_sigs: Vec<Vec<u8>>,
}

fn parse_hybrid_envelope(bytes: &[u8]) -> Result<HybridEnvelope, String> {
    const MAYO2_SIG_BYTES: usize = 186;
    const MIN_LEN: usize = 1 + 48 + 8 + 1;
    if bytes.len() < MIN_LEN {
        return Err(format!(
            "hybrid envelope too short: {} bytes (min {MIN_LEN})",
            bytes.len()
        ));
    }
    if bytes[0] != 0x01 {
        return Err(format!("expected 0x01 hybrid tag, got {:#04x}", bytes[0]));
    }
    let classical = bytes[1..49].to_vec();
    let participants = u64::from_be_bytes(bytes[49..57].try_into().unwrap()) as usize;
    if participants == 0 || participants > 4096 {
        return Err(format!("implausible participant count: {participants}"));
    }
    let bitmap_bytes = participants.div_ceil(8);
    if bytes.len() < 57 + bitmap_bytes {
        return Err("signer bitmap truncated".to_string());
    }
    let bitmap = &bytes[57..57 + bitmap_bytes];
    let signers: Vec<usize> = (0..participants)
        .filter(|i| (bitmap[i / 8] >> (i % 8)) & 1 == 1)
        .collect();
    let rest = &bytes[57 + bitmap_bytes..];
    if rest.len() != signers.len() * MAYO2_SIG_BYTES {
        return Err(format!(
            "MAYO2 block length mismatch: {} bytes for {} signer(s) (expected {})",
            rest.len(),
            signers.len(),
            signers.len() * MAYO2_SIG_BYTES
        ));
    }
    let mayo_sigs = signers
        .iter()
        .enumerate()
        .map(|(i, _)| rest[i * MAYO2_SIG_BYTES..(i + 1) * MAYO2_SIG_BYTES].to_vec())
        .collect();
    Ok(HybridEnvelope {
        classical,
        participants,
        signers,
        mayo_sigs,
    })
}

/// Verifies the PQ half per PQ_PROTOCOL_AUTH.md §4: every carried MAYO2
/// signature must verify and the count must reach the BLS `threshold_required`
/// (Phase 2 sets pq_quorum = threshold_required). The bitmap indexes the
/// ascending-sorted Ed25519 participant set, while `validator_mayo_public_keys`
/// is in validator_index order — reorder before use.
fn verify_mayo_half(
    envelope: &HybridEnvelope,
    keys: &KeysFile,
    namespace: &[u8],
    message: &[u8],
) -> Result<usize, String> {
    let ed = keys
        .validator_public_keys
        .as_ref()
        .ok_or("keys file lacks validator_public_keys")?;
    let mayo = keys
        .validator_mayo_public_keys
        .as_ref()
        .ok_or("keys file lacks validator_mayo_public_keys")?;
    if ed.len() != mayo.len() || ed.is_empty() {
        return Err("validator_public_keys/validator_mayo_public_keys mismatch".to_string());
    }
    let ed_bytes: Result<Vec<Vec<u8>>, _> = ed
        .iter()
        .map(|h| hex::decode(h.trim()).map_err(|e| format!("bad ed25519 key hex: {e}")))
        .collect();
    let ed_bytes = ed_bytes?;
    // Participant index = rank in the ascending Ed25519 key order.
    let mut validator_by_participant: Vec<usize> = (0..ed_bytes.len()).collect();
    validator_by_participant.sort_by_key(|&vi| ed_bytes[vi].clone());
    let participant_mayo: Result<Vec<Vec<u8>>, String> = validator_by_participant
        .iter()
        .map(|&vi| {
            hex::decode(mayo[vi].trim())
                .map_err(|e| format!("bad MAYO2 key hex for validator {vi}: {e}"))
        })
        .collect();
    let participant_mayo = participant_mayo?;
    for (rank, pk) in participant_mayo.iter().enumerate() {
        if pk.len() != Mayo2::PK_BYTES {
            return Err(format!(
                "MAYO2 key for participant {rank} has {} bytes (expected {})",
                pk.len(),
                Mayo2::PK_BYTES
            ));
        }
    }
    if envelope.participants != participant_mayo.len() {
        return Err(format!(
            "bitmap covers {} participants but the key file has {}",
            envelope.participants,
            participant_mayo.len()
        ));
    }
    let mut namespaced = Vec::with_capacity(namespace.len() + message.len());
    namespaced.extend_from_slice(namespace);
    namespaced.extend_from_slice(message);
    for (pos, &signer) in envelope.signers.iter().enumerate() {
        let ok = junoclaw_mayo_verify::verify::<Mayo2>(
            &namespaced,
            &envelope.mayo_sigs[pos],
            &participant_mayo[signer],
        )
        .map_err(|e| format!("MAYO2 verify error for participant {signer}: {e}"))?;
        if !ok {
            return Err(format!("MAYO2 signature for participant {signer} FAILED"));
        }
    }
    Ok(envelope.signers.len())
}

/// Decode the signer index set out of a certificate's bytes.
/// Hybrid envelopes carry the BitMap after `0x01 + 48B BLS`; classical
/// certificates place it directly after the 48B signature. Returns
/// (signer indices, participant count).
fn cert_signers(bytes: &[u8]) -> Result<(Vec<usize>, usize), String> {
    let off = if !bytes.is_empty() && bytes[0] == 0x01 { 1 } else { 0 };
    if bytes.len() < off + 48 + 8 + 1 {
        return Err(format!("cert too short: {} bytes", bytes.len()));
    }
    let participants =
        u64::from_be_bytes(bytes[off + 48..off + 56].try_into().unwrap()) as usize;
    if participants == 0 || participants > 4096 {
        return Err(format!("implausible participant count: {participants}"));
    }
    let bm = off + 56;
    let bm_len = participants.div_ceil(8);
    if bytes.len() < bm + bm_len {
        return Err("signer bitmap truncated".to_string());
    }
    let bitmap = &bytes[bm..bm + bm_len];
    Ok((
        (0..participants)
            .filter(|i| (bitmap[i / 8] >> (i % 8)) & 1 == 1)
            .collect(),
        participants,
    ))
}

/// Remove ANSI SGR/CSI escape sequences so `certificate=` values parse.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                Some('[') => {
                    for c2 in chars.by_ref() {
                        if c2.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                _ => continue,
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Pull hex certificate strings out of a log line containing `certificate=`.
fn extract_certs(line: &str) -> Vec<String> {
    line.split("certificate=")
        .skip(1)
        .filter_map(|rest| {
            let hex: String = rest
                .chars()
                .take_while(|c| c.is_ascii_hexdigit())
                .collect();
            (hex.len() >= 114).then_some(hex)
        })
        .collect()
}

/// `verify-cert certs`: tally signer bitmaps across finalization-certificate
/// log lines. Method credit: 79anvi's bitmap reader (G2 runsheet v5).
/// Input: --log FILE (repeatable, or stdin). --last N keeps the newest N
/// certs per source; --json emits machine-readable output.
fn run_certs(args: &[String]) {
    let mut logs: Vec<String> = Vec::new();
    let mut last_n: Option<usize> = None;
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--log" => {
                i += 1;
                if i < args.len() {
                    logs.push(args[i].clone());
                }
            }
            "--last" => {
                i += 1;
                last_n = args.get(i).and_then(|s| s.parse().ok());
            }
            "--json" => json = true,
            _ => {
                eprintln!("Unknown certs argument: {}", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let sources: Vec<(String, String)> = if logs.is_empty() {
        use std::io::Read as _;
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            eprintln!("INVALID: provide --log <file> or pipe a log to stdin");
            process::exit(1);
        }
        vec![("stdin".to_string(), buf)]
    } else {
        let mut v = Vec::new();
        for p in &logs {
            match std::fs::read_to_string(p) {
                Ok(c) => v.push((p.clone(), c)),
                Err(e) => {
                    eprintln!("INVALID: cannot read log '{p}': {e}");
                    process::exit(2);
                }
            }
        }
        v
    };

    let mut reports: Vec<serde_json::Value> = Vec::new();
    for (name, text) in &sources {
        let mut certs: Vec<Vec<usize>> = Vec::new();
        let mut participants = 0usize;
        let mut bad = 0usize;
        for line in text.lines() {
            for hex in extract_certs(&strip_ansi(line)) {
                match hex::decode(&hex)
                    .map_err(|e| e.to_string())
                    .and_then(|b| cert_signers(&b))
                {
                    Ok((signers, nparts)) => {
                        participants = nparts;
                        certs.push(signers);
                    }
                    Err(_) => bad += 1,
                }
            }
        }
        if let Some(n) = last_n {
            let skip = certs.len().saturating_sub(n);
            certs.drain(..skip);
        }
        let n = certs.len();
        let mut counts = vec![0usize; participants];
        let mut masks: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for signers in &certs {
            let mut mask = 0u64;
            for &s in signers {
                if s < participants {
                    counts[s] += 1;
                }
                mask |= 1u64 << s.min(63);
            }
            *masks.entry(format!("{mask:#x}")).or_default() += 1;
        }
        if json {
            reports.push(serde_json::json!({
                "source": name,
                "certs": n,
                "participants": participants,
                "per_index": counts,
                "masks": masks,
                "parse_failures": bad,
            }));
            continue;
        }
        println!("=== {name} ===");
        if n == 0 {
            println!("  no finalization certificates parsed ({bad} malformed lines skipped)");
            continue;
        }
        println!("  {n} certificate(s), {participants} participants");
        for (i, &c) in counts.iter().enumerate() {
            println!(
                "  index {i}: {c}/{n} ({:.0}%){}",
                c as f64 * 100.0 / n as f64,
                if c == 0 { "  NEVER SIGNS" } else { "" }
            );
        }
        let mask_list: Vec<String> = masks
            .iter()
            .map(|(m, c)| format!("{m}×{c}"))
            .collect();
        println!("  masks: {}", mask_list.join(" "));
        if bad > 0 {
            println!("  {bad} malformed certificate line(s) skipped");
        }
        println!();
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&reports).unwrap());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        process::exit(0);
    }

    // Subcommand: `verify-cert certs --log <file> [--log <file>...]`
    if args[1] == "certs" {
        run_certs(&args[2..]);
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

    // Hybrid certificates (PQ_PROTOCOL_AUTH.md §5) wrap the classical BLS
    // cert in a version-tagged envelope. Unwrap it so the classical half
    // verifies below, and keep the envelope for the optional PQ check.
    let (classical_bytes, envelope) = if cert_bytes.len() == 48 {
        (cert_bytes.clone(), None)
    } else if !cert_bytes.is_empty() && cert_bytes[0] == 0x01 {
        match parse_hybrid_envelope(&cert_bytes) {
            Ok(env) => {
                println!(
                    "Hybrid envelope: {} bytes = tag 0x01 + classical {} B + bitmap ({} participants, signers {:?}) + {} x 186 B MAYO2",
                    cert_bytes.len(),
                    env.classical.len(),
                    env.participants,
                    env.signers,
                    env.mayo_sigs.len()
                );
                (env.classical.clone(), Some(env))
            }
            Err(e) => {
                eprintln!("INVALID: Failed to parse hybrid certificate envelope: {e}");
                process::exit(3);
            }
        }
    } else {
        // Unknown format: fall through — the G1 decode below reports it.
        (cert_bytes.clone(), None)
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
    let mut parsed_keys: Option<KeysFile> = None;
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
        let b = match hex::decode(keys.threshold_public_key_hex.trim()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("INVALID: Failed to decode threshold_public_key_hex from '{path}': {e}");
                process::exit(2);
            }
        };
        parsed_keys = Some(keys);
        b
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
    // Certificate<MinSig> wraps a Lazy<G1> (48-byte compressed G1 point);
    // hybrid envelopes were unwrapped above into `classical_bytes`.
    let signature = match G1::decode(bytes::Bytes::from(classical_bytes.clone())) {
        Ok(sig) => sig,
        Err(e) => {
            eprintln!("INVALID: Failed to decode certificate as G1 signature: {e}");
            eprintln!("  Expected 48-byte compressed G1 point (MinSig signature variant)");
            eprintln!("  Got {} bytes: {}", classical_bytes.len(), hex::encode(&classical_bytes));
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

            // PQ half — only hybrid certificates carry it. Verified when the
            // keys file carries the committee's MAYO2 keys (PQ_PROTOCOL_AUTH.md §4:
            // every carried signature must verify and the count must reach
            // `threshold_required`, the Phase 2 pq_quorum).
            if let Some(env) = &envelope {
                match parsed_keys.as_ref() {
                    Some(keys) => match verify_mayo_half(env, keys, &namespace_bytes, &message_bytes) {
                        Ok(valid) => {
                            let quorum = keys.threshold_required.unwrap_or(0);
                            if valid >= quorum {
                                println!(
                                    "PQ half: {valid} carried MAYO2 signature(s) all valid, quorum {quorum} met — PQ_PROTOCOL_AUTH §4 satisfied"
                                );
                            } else {
                                eprintln!(
                                    "INVALID: PQ quorum NOT met: {valid} < {quorum} (PQ_PROTOCOL_AUTH §4)"
                                );
                                process::exit(4);
                            }
                        }
                        Err(e) => {
                            eprintln!("INVALID: PQ half verification failed: {e}");
                            process::exit(4);
                        }
                    },
                    None => {
                        println!("PQ half: NOT verified (no keys file with MAYO2 keys provided)");
                    }
                }
            }

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
    println!("  verify-cert certs [--log <file> ...] [--last N] [--json]");
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
    println!("");
    println!("  # Signer-bitmap tally over finalization certs in a node log");
    println!("  # (or pipe: journalctl -u junoclaw --since -3min -o cat | verify-cert certs):");
    println!("  verify-cert certs --log /var/log/slay3rd.log --last 500");
    println!("  verify-cert certs --log seat-a.log --log seat-b.log   # cross-seat");
}
