//! hybrid-sign — hybrid secp256k1+MAYO transaction signer for JunoClaw.
//!
//! Produces raw `TxRaw` bytes (hex) carrying a `HybridSecp256k1Mayo` pubkey in
//! `SignerInfo.public_key` and a packed signature `[64B secp256k1 | MAYO sig]`
//! over `sha256(SignDoc)`. Broadcast with:
//!   tx-sender broadcast --tx-hex <hex>
//!   tx-sender broadcast --tx-file <path>
//!
//! Linux/Docker only: `sriracha-mayo` wraps the MAYO-C reference
//! implementation and needs cmake + a C toolchain.
//!
//! Subcommands:
//!   keygen --variant <mayo1|mayo2|mayo3|mayo5>
//!   sign   --variant <v> --secp-sk <hex> --mayo-sk <hex>
//!          --to <juno...> --amount <N> [--denom ujclaw]
//!          --sequence <N> [--chain-id junoclaw-1] [--account-number 17]
//!          [--gas 5000000] [--fee-amount 100000] [--memo <s>]
//!          [--out <path>]

use std::process::exit;

use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature as SecpSig, SigningKey};
use layer_proto::cosmos::bank::v1beta1::MsgSend;
use layer_proto::cosmos::base::v1beta1::Coin;
use layer_proto::cosmos::tx::signing::v1beta1::SignMode;
use layer_proto::cosmos::tx::v1beta1::{
    mode_info, AuthInfo, Fee, ModeInfo, SignDoc, SignerInfo, TxBody, TxRaw,
};
use layer_proto::google::protobuf::Any;
use layer_std::{MayoVariant, PubKey, HYBRID_PUBKEY_TYPE_URL};
use prost::Message;
use sha2::{Digest, Sha256};

const DEFAULT_CHAIN_ID: &str = "junoclaw-1";
const DEFAULT_ACCOUNT_NUMBER: u64 = 17;
const DEFAULT_DENOM: &str = "ujclaw";
const DEFAULT_GAS_LIMIT: u64 = 5_000_000;
const DEFAULT_FEE_AMOUNT: &str = "100000";

fn print_usage() {
    eprintln!("Usage: hybrid-sign <subcommand> [flags]");
    eprintln!();
    eprintln!("Subcommands:");
    eprintln!("  keygen   Generate a secp256k1 + MAYO keypair, print hybrid address");
    eprintln!("  sign     Sign a MsgSend, print hex-encoded TxRaw bytes");
    eprintln!();
    eprintln!("keygen flags:");
    eprintln!("  --variant <mayo1|mayo2|mayo3|mayo5>   MAYO parameter set (required)");
    eprintln!();
    eprintln!("sign flags:");
    eprintln!("  --variant <v>          MAYO parameter set (required)");
    eprintln!("  --secp-sk <hex>        secp256k1 secret key, 32 bytes (required)");
    eprintln!("  --mayo-sk <hex>        MAYO compact secret-key seed (required)");
    eprintln!("  --to <addr>            Recipient juno address (required)");
    eprintln!("  --amount <N>           Amount to send (required)");
    eprintln!("  --sequence <N>         Account sequence (required)");
    eprintln!("  --denom <str>          Denom (default: ujclaw)");
    eprintln!("  --chain-id <str>       Chain id (default: junoclaw-1)");
    eprintln!("  --account-number <N>   Account number (default: 17)");
    eprintln!("  --gas <N>              Gas limit (default: 5000000)");
    eprintln!("  --fee-amount <N>       Fee amount in --denom (default: 100000)");
    eprintln!("  --memo <str>           Tx memo (default: empty)");
    eprintln!("  --out <path>           Write hex tx to file instead of stdout");
}

#[derive(Default)]
struct Flags {
    variant: Option<String>,
    secp_sk: Option<String>,
    mayo_sk: Option<String>,
    to: Option<String>,
    amount: Option<String>,
    denom: String,
    chain_id: String,
    account_number: u64,
    sequence: Option<u64>,
    gas: u64,
    fee_amount: String,
    memo: String,
    out: Option<String>,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut f = Flags {
        denom: DEFAULT_DENOM.to_string(),
        chain_id: DEFAULT_CHAIN_ID.to_string(),
        account_number: DEFAULT_ACCOUNT_NUMBER,
        gas: DEFAULT_GAS_LIMIT,
        fee_amount: DEFAULT_FEE_AMOUNT.to_string(),
        ..Default::default()
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--variant" => f.variant = Some(next_val(args, &mut i, "--variant")?),
            "--secp-sk" => f.secp_sk = Some(next_val(args, &mut i, "--secp-sk")?),
            "--mayo-sk" => f.mayo_sk = Some(next_val(args, &mut i, "--mayo-sk")?),
            "--to" => f.to = Some(next_val(args, &mut i, "--to")?),
            "--amount" => f.amount = Some(next_val(args, &mut i, "--amount")?),
            "--denom" => f.denom = next_val(args, &mut i, "--denom")?,
            "--chain-id" => f.chain_id = next_val(args, &mut i, "--chain-id")?,
            "--account-number" => {
                f.account_number = next_val(args, &mut i, "--account-number")?
                    .parse()
                    .map_err(|_| "invalid --account-number".to_string())?
            }
            "--sequence" => {
                f.sequence = Some(
                    next_val(args, &mut i, "--sequence")?
                        .parse()
                        .map_err(|_| "invalid --sequence".to_string())?,
                )
            }
            "--gas" => {
                f.gas = next_val(args, &mut i, "--gas")?
                    .parse()
                    .map_err(|_| "invalid --gas".to_string())?
            }
            "--fee-amount" => f.fee_amount = next_val(args, &mut i, "--fee-amount")?,
            "--memo" => f.memo = next_val(args, &mut i, "--memo")?,
            "--out" => f.out = Some(next_val(args, &mut i, "--out")?),
            flag => return Err(format!("unknown flag: {}", flag)),
        }
        i += 1;
    }
    Ok(f)
}

fn next_val(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| format!("{} requires a value", name))
}

fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err("hex string has odd length".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn parse_variant(s: &str) -> Result<MayoVariant, String> {
    match s {
        "mayo1" | "1" => Ok(MayoVariant::Mayo1),
        "mayo2" | "2" => Ok(MayoVariant::Mayo2),
        "mayo3" | "3" => Ok(MayoVariant::Mayo3),
        "mayo5" | "5" => Ok(MayoVariant::Mayo5),
        other => Err(format!("unknown variant: {} (expected mayo1|mayo2|mayo3|mayo5)", other)),
    }
}

/// Generate a MAYO keypair for `variant` from `rng`, returning
/// (secret_seed_bytes, public_key_bytes, variant_tag).
fn mayo_keygen(
    variant: MayoVariant,
    rng: &mut impl rand_core::CryptoRngCore,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    macro_rules! gen {
        ($p:ty) => {{
            let (pk, sk) = sriracha_mayo::SecretKey::<$p>::random(rng)
                .map_err(|e| format!("mayo keygen failed: {:?}", e))?;
            (
                AsRef::<[u8]>::as_ref(&sk).to_vec(),
                AsRef::<[u8]>::as_ref(&pk).to_vec(),
            )
        }};
    }
    Ok(match variant {
        MayoVariant::Mayo1 => gen!(sriracha_mayo::Mayo1),
        MayoVariant::Mayo2 => gen!(sriracha_mayo::Mayo2),
        MayoVariant::Mayo3 => gen!(sriracha_mayo::Mayo3),
        MayoVariant::Mayo5 => gen!(sriracha_mayo::Mayo5),
    })
}

/// Restore the MAYO public key from its compact secret seed.
/// `from_seed` derives the public key deterministically — no signing needed.
fn mayo_pk_from_seed(variant: MayoVariant, seed: &[u8]) -> Result<Vec<u8>, String> {
    macro_rules! pk {
        ($p:ty) => {{
            let (pk, _sk) = sriracha_mayo::SecretKey::<$p>::from_seed(seed)
                .map_err(|e| format!("invalid mayo secret seed: {:?}", e))?;
            AsRef::<[u8]>::as_ref(&pk).to_vec()
        }};
    }
    Ok(match variant {
        MayoVariant::Mayo1 => pk!(sriracha_mayo::Mayo1),
        MayoVariant::Mayo2 => pk!(sriracha_mayo::Mayo2),
        MayoVariant::Mayo3 => pk!(sriracha_mayo::Mayo3),
        MayoVariant::Mayo5 => pk!(sriracha_mayo::Mayo5),
    })
}

/// MAYO-sign `message_hash` with the compact secret seed; returns detached sig
/// bytes. The chain verifies this against the same 32-byte sha256(SignDoc) hash.
fn mayo_sign(variant: MayoVariant, seed: &[u8], message_hash: &[u8]) -> Result<Vec<u8>, String> {
    macro_rules! sign {
        ($p:ty) => {{
            let (_pk, sk) = sriracha_mayo::SecretKey::<$p>::from_seed(seed)
                .map_err(|e| format!("invalid mayo secret seed: {:?}", e))?;
            let sig = sk
                .sign(message_hash)
                .map_err(|e| format!("mayo signing failed: {:?}", e))?;
            AsRef::<[u8]>::as_ref(&sig).to_vec()
        }};
    }
    Ok(match variant {
        MayoVariant::Mayo1 => sign!(sriracha_mayo::Mayo1),
        MayoVariant::Mayo2 => sign!(sriracha_mayo::Mayo2),
        MayoVariant::Mayo3 => sign!(sriracha_mayo::Mayo3),
        MayoVariant::Mayo5 => sign!(sriracha_mayo::Mayo5),
    })
}

fn cmd_keygen(variant: MayoVariant) -> Result<(), String> {
    let mut rng = rand_core::OsRng;

    let secp_sk = SigningKey::random(&mut rng);
    let secp_pk = secp_sk.verifying_key().to_sec1_bytes().to_vec();
    let (mayo_sk, mayo_pk) = mayo_keygen(variant, &mut rng)?;

    let pk = PubKey::hybrid(secp_pk, variant, mayo_pk);
    let address = pk
        .account_id()
        .map_err(|e| format!("account id: {}", e))?
        .to_string();

    eprintln!("=== KEEP THESE SECRET ===");
    println!("secp_sk={}", hex::encode(secp_sk.to_bytes()));
    println!("mayo_sk={}", hex::encode(mayo_sk));
    eprintln!("=== PUBLIC ===");
    println!("hybrid_pubkey={}", hex::encode(pk.to_hybrid_any_bytes().unwrap()));
    println!("address={}", address);
    Ok(())
}

fn cmd_sign(f: Flags) -> Result<(), String> {
    let variant = parse_variant(f.variant.as_deref().ok_or("--variant is required")?)?;
    let secp_sk_bytes = decode_hex(f.secp_sk.as_deref().ok_or("--secp-sk is required")?)?;
    let mayo_seed = decode_hex(f.mayo_sk.as_deref().ok_or("--mayo-sk is required")?)?;
    let to = f.to.ok_or("--to is required")?;
    let amount = f.amount.ok_or("--amount is required")?;
    let sequence = f.sequence.ok_or("--sequence is required")?;

    let secp_sk =
        SigningKey::from_slice(&secp_sk_bytes).map_err(|e| format!("invalid secp-sk: {}", e))?;
    let secp_pk = secp_sk.verifying_key().to_sec1_bytes().to_vec();
    let mayo_pk = mayo_pk_from_seed(variant, &mayo_seed)?;

    // Hybrid pubkey + sender address (domain-separated derivation, matches chain).
    let pk = PubKey::hybrid(secp_pk, variant, mayo_pk);
    let from = pk
        .account_id()
        .map_err(|e| format!("account id: {}", e))?
        .to_string();
    let pubkey_any_value = pk
        .to_hybrid_any_bytes()
        .ok_or("failed to encode hybrid pubkey")?;

    // TxBody: single MsgSend from the hybrid account.
    let msg = MsgSend {
        from_address: from.clone(),
        to_address: to,
        amount: vec![Coin {
            denom: f.denom.clone(),
            amount,
        }],
    };
    let msg_any = Any {
        type_url: "/cosmos.bank.v1beta1.MsgSend".to_string(),
        value: msg.encode_to_vec(),
    };
    let tx_body = TxBody {
        messages: vec![msg_any],
        memo: f.memo,
        timeout_height: 0,
        extension_options: vec![],
        non_critical_extension_options: vec![],
    };
    let body_bytes = tx_body.encode_to_vec();

    // AuthInfo: hybrid pubkey + SIGN_MODE_DIRECT + fee.
    let signer_info = SignerInfo {
        public_key: Some(Any {
            type_url: HYBRID_PUBKEY_TYPE_URL.to_string(),
            value: pubkey_any_value,
        }),
        mode_info: Some(ModeInfo {
            sum: Some(mode_info::Sum::Single(mode_info::Single {
                mode: SignMode::Direct as i32,
            })),
        }),
        sequence,
    };
    let auth_info = AuthInfo {
        signer_infos: vec![signer_info],
        fee: Some(Fee {
            amount: vec![Coin {
                denom: f.denom.clone(),
                amount: f.fee_amount,
            }],
            gas_limit: f.gas,
            payer: String::new(),
            granter: String::new(),
        }),
        tip: None,
    };
    let auth_info_bytes = auth_info.encode_to_vec();

    // SignDoc -> sha256 -> message_hash (matches HashableMessage::hash_direct_mode).
    let sign_doc = SignDoc {
        body_bytes: body_bytes.clone(),
        auth_info_bytes: auth_info_bytes.clone(),
        chain_id: f.chain_id,
        account_number: f.account_number,
    };
    let sign_doc_bytes = sign_doc.encode_to_vec();
    let message_hash: [u8; 32] = Sha256::digest(&sign_doc_bytes).into();

    // secp256k1: sign the 32-byte hash, emit 64-byte r||s compact signature.
    let secp_sig: SecpSig = secp_sk
        .sign_prehash(&message_hash)
        .map_err(|e| format!("secp signing failed: {}", e))?;
    let secp_sig_bytes = secp_sig.to_bytes().to_vec();

    // MAYO: sign the same hash; pack [64B secp | mayo sig].
    let mayo_sig = mayo_sign(variant, &mayo_seed, &message_hash)?;
    let packed = PubKey::pack_hybrid_signature(&secp_sig_bytes, &mayo_sig);

    // TxRaw carries the same byte encodings used in the SignDoc.
    let tx_raw = TxRaw {
        body_bytes,
        auth_info_bytes,
        signatures: vec![packed.to_vec()],
    };
    let tx_bytes = tx_raw.encode_to_vec();

    eprintln!("sender: {}", from);
    eprintln!("tx size: {} bytes", tx_bytes.len());
    eprintln!("txhash: {}", hex::encode(Sha256::digest(&tx_bytes)));

    let hex_out = hex::encode(&tx_bytes);
    match f.out {
        Some(path) => {
            std::fs::write(&path, &hex_out).map_err(|e| format!("write {}: {}", path, e))?;
            eprintln!("wrote {}", path);
        }
        None => println!("{}", hex_out),
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        exit(0);
    }
    let result = match args[0].as_str() {
        "keygen" => {
            let f = parse_flags(&args[1..]).unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                exit(1);
            });
            match f.variant.as_deref().map(parse_variant) {
                Some(Ok(v)) => cmd_keygen(v),
                Some(Err(e)) => Err(e),
                None => Err("--variant is required".to_string()),
            }
        }
        "sign" => parse_flags(&args[1..]).and_then(cmd_sign),
        other => Err(format!("unknown subcommand: {}", other)),
    };
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        exit(1);
    }
}
