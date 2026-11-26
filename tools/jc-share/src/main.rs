//! jc-share — sealed bundle exchange for ceremonies (Phase 2, sovereign
//! ceremony transfer plan).
//!
//! Server: content-addressed blob relay — stores ONLY ciphertext keyed by
//! sha256, one-time fetch tokens, TTL sweeper, append-only access log.
//!   jc-share serve --dir blobs --bind 127.0.0.1:8088 [--put-token T] \
//!                  [--access-log access.log]
//!
//! Client:
//!   jc-share seal   --to <ed25519-pubhex> --in <file> --out <file.sealed>
//!   jc-share unseal --seed <hex|@file>    --in <file.sealed> --out <file>
//!   jc-share publish --relay <base-url> --file <blob> [--ttl-secs N] \
//!                  [--downloads N] [--put-token T]
//!   jc-share fetch <url> --out <file> [--unseal --seed <hex|@file>]
//!
//! Seal format is EncField JSON — the same x25519-xchacha20poly1305-v1
//! envelope as generate-testnet-keys dealing-priv files. `seal` uses the
//! "junoclaw-share-v1" key domain; `unseal` tries share then dealing
//! domains so it can open both.

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEME: &str = "x25519-xchacha20poly1305-v1";
const SCHEME_V2: &str = "x25519-mlkem768-xchacha20poly1305-v2";
const DOMAIN_SHARE: &[u8] = b"junoclaw-share-v1";
const DOMAIN_DEALING: &[u8] = b"junoclaw-dealing-v1";
const DOMAIN_KEM_SEED: &[u8] = b"junoclaw-kem-v1";

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn sha256_hex(b: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(b))
}

// ---------------------------------------------------------------- seal ----

#[derive(Serialize, Deserialize)]
struct EncField {
    scheme: String,
    ephem_hex: String,
    nonce_hex: String,
    ct_hex: String,
    #[serde(default)]
    kem_ct_hex: Option<String>,
}

// Deterministic RNG seeded from SHA-256 — used ONLY for reproducing a
// recipient's ML-KEM keypair from their seed (keygen is the one place a
// deterministic stream is correct). Never used for ephemeral randomness.
struct SeedRng {
    state: [u8; 32],
    ctr: u64,
    buf: [u8; 32],
    pos: usize,
}

impl SeedRng {
    fn new(state: [u8; 32]) -> Self {
        SeedRng { state, ctr: 0, buf: [0u8; 32], pos: 32 }
    }
    fn next_block(&mut self) {
        self.buf = sha2::Sha256::new()
            .chain_update(self.state)
            .chain_update(self.ctr.to_le_bytes())
            .finalize()
            .into();
        self.ctr += 1;
        self.pos = 0;
    }
}

impl rand::RngCore for SeedRng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for b in dest.iter_mut() {
            if self.pos == 32 {
                self.next_block();
            }
            *b = self.buf[self.pos];
            self.pos += 1;
        }
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl rand::CryptoRng for SeedRng {}

fn kem_keys_from_seed(
    seed: &[u8],
) -> (
    ml_kem::kem::DecapsulationKey<ml_kem::MlKem768Params>,
    ml_kem::kem::EncapsulationKey<ml_kem::MlKem768Params>,
) {
    use ml_kem::KemCore;
    let state: [u8; 32] = sha2::Sha256::new()
        .chain_update(DOMAIN_KEM_SEED)
        .chain_update(seed)
        .finalize()
        .into();
    ml_kem::MlKem768::generate(&mut SeedRng::new(state))
}

fn x25519_scalar_from_ed_seed(seed: &[u8]) -> [u8; 32] {
    let h = sha2::Sha512::digest(seed);
    let mut s = [0u8; 32];
    s.copy_from_slice(&h[..32]);
    s[0] &= 248;
    s[31] &= 127;
    s[31] |= 64;
    s
}

fn x25519_pub_from_ed_pub(ed: &[u8]) -> [u8; 32] {
    use curve25519_dalek_ng::edwards::CompressedEdwardsY;
    let point = CompressedEdwardsY(ed.try_into().expect("ed pubkey must be 32 bytes"))
        .decompress()
        .expect("recipient ed25519 pubkey is not on the curve");
    point.to_montgomery().0
}

fn aead_key(domain: &[u8], shared: &[u8; 32]) -> [u8; 32] {
    let mut k = [0u8; 32];
    k.copy_from_slice(
        &sha2::Sha256::new().chain_update(domain).chain_update(shared).finalize(),
    );
    k
}

// v2: key = sha256(domain || x25519_shared || mlkem_shared) — recovering the
// plaintext requires breaking BOTH KEMs. A harvest-now/decrypt-later attacker
// with a quantum machine still cannot open the envelope.
fn aead_key_v2(domain: &[u8], x_shared: &[u8; 32], kem_shared: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 32];
    k.copy_from_slice(
        &sha2::Sha256::new()
            .chain_update(domain)
            .chain_update(x_shared)
            .chain_update(kem_shared)
            .finalize(),
    );
    k
}

fn seal_bytes(domain: &[u8], recipient_ed_hex: &str, plaintext: &[u8]) -> EncField {
    seal_inner(domain, recipient_ed_hex, None, plaintext)
}

fn seal_inner(
    domain: &[u8],
    recipient_ed_hex: &str,
    recipient_kem_hex: Option<&str>,
    plaintext: &[u8],
) -> EncField {
    use chacha20poly1305::aead::{Aead, KeyInit};
    let ed = hex::decode(recipient_ed_hex).expect("bad recipient ed25519 hex");
    let recip = x25519_dalek::PublicKey::from(x25519_pub_from_ed_pub(&ed));
    let eph = x25519_dalek::StaticSecret::random_from_rng(OsRng);
    let shared = eph.diffie_hellman(&recip);

    let (key, scheme, kem_ct_hex) = match recipient_kem_hex {
        Some(kem_hex) => {
            use ml_kem::kem::Encapsulate;
            use ml_kem::EncodedSizeUser;
            let ek_bytes = hex::decode(kem_hex).expect("bad recipient kem pub hex");
            let ek_arr: &ml_kem::Encoded<
                ml_kem::kem::EncapsulationKey<ml_kem::MlKem768Params>,
            > = ek_bytes
                .as_slice()
                .try_into()
                .expect("bad recipient kem pub length");
            let ek = ml_kem::kem::EncapsulationKey::<ml_kem::MlKem768Params>::from_bytes(ek_arr);
            let (kem_ct, kem_ss) = ek
                .encapsulate(&mut rand::rngs::OsRng)
                .expect("mlkem encapsulate failed");
            (
                aead_key_v2(domain, shared.as_bytes(), &kem_ss),
                SCHEME_V2,
                Some(hex::encode(kem_ct.as_slice())),
            )
        }
        None => (aead_key(domain, shared.as_bytes()), SCHEME, None),
    };

    let cipher = chacha20poly1305::XChaCha20Poly1305::new_from_slice(&key).expect("key length");
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(chacha20poly1305::XNonce::from_slice(&nonce), plaintext)
        .expect("aead encrypt");
    EncField {
        scheme: scheme.to_string(),
        ephem_hex: hex::encode(x25519_dalek::PublicKey::from(&eph).to_bytes()),
        nonce_hex: hex::encode(nonce),
        ct_hex: hex::encode(ct),
        kem_ct_hex,
    }
}

fn unseal_with(domain: &[u8], enc: &EncField, my_seed: &[u8]) -> Option<Vec<u8>> {
    use chacha20poly1305::aead::{Aead, KeyInit};
    let me = x25519_dalek::StaticSecret::from(x25519_scalar_from_ed_seed(my_seed));
    let eph_bytes: [u8; 32] = hex::decode(&enc.ephem_hex).ok()?.try_into().ok()?;
    let shared = me.diffie_hellman(&x25519_dalek::PublicKey::from(eph_bytes));

    let key = if enc.scheme == SCHEME_V2 {
        use ml_kem::kem::Decapsulate;
        let (dk, _ek) = kem_keys_from_seed(my_seed);
        let ct_bytes = hex::decode(enc.kem_ct_hex.as_deref()?).ok()?;
        let kem_ct: ml_kem::Ciphertext<ml_kem::MlKem768> =
            ct_bytes.as_slice().try_into().ok()?;
        let kem_ss = dk.decapsulate(&kem_ct).ok()?;
        aead_key_v2(domain, shared.as_bytes(), &kem_ss)
    } else {
        aead_key(domain, shared.as_bytes())
    };

    let cipher = chacha20poly1305::XChaCha20Poly1305::new_from_slice(&key).ok()?;
    let nonce = hex::decode(&enc.nonce_hex).ok()?;
    let ct = hex::decode(&enc.ct_hex).ok()?;
    cipher
        .decrypt(chacha20poly1305::XNonce::from_slice(&nonce), ct.as_ref())
        .ok()
}

fn unseal_bytes(enc: &EncField, my_seed: &[u8]) -> Vec<u8> {
    assert!(
        enc.scheme == SCHEME || enc.scheme == SCHEME_V2,
        "unknown seal scheme"
    );
    unseal_with(DOMAIN_SHARE, enc, my_seed)
        .or_else(|| unseal_with(DOMAIN_DEALING, enc, my_seed))
        .expect("unseal failed — wrong seed, or file not addressed to this key")
}

fn load_seed(arg: &str) -> Vec<u8> {
    // "--seed <64-hex>" or "--seed @/path/to/file" (file holds raw seed hex).
    let hex_str = if let Some(path) = arg.strip_prefix('@') {
        String::from_utf8(std::fs::read(path).expect("seed file unreadable"))
            .expect("seed file not utf8")
            .trim()
            .to_string()
    } else {
        arg.trim().to_string()
    };
    hex::decode(&hex_str).expect("seed must be 32-byte hex")
}

// --------------------------------------------------------------- server ---

#[derive(Serialize, Deserialize)]
struct Meta {
    expires: u64,
    token_sha: String,
    max_downloads: u32,
    downloads: u32,
}

struct Srv {
    dir: PathBuf,
    put_token: Option<String>,
    access_log: PathBuf,
    max_blob: usize,
    max_ttl: u64,
    default_ttl: u64,
    default_downloads: u32,
}

impl Srv {
    fn blob_path(&self, hash: &str) -> PathBuf {
        self.dir.join(format!("{hash}.blob"))
    }
    fn meta_path(&self, hash: &str) -> PathBuf {
        self.dir.join(format!("{hash}.meta"))
    }
    fn log(&self, op: &str, hash: &str, result: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.access_log)
        {
            let _ = writeln!(f, "{} {op} {hash} {result}", unix_now());
        }
    }
    fn sweep(&self) -> usize {
        let mut swept = 0;
        let now = unix_now();
        if let Ok(rd) = self.dir.read_dir() {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map_or(true, |x| x != "meta") {
                    continue;
                }
                if let Ok(m) = std::fs::read_to_string(&p)
                    .ok()
                    .and_then(|s| serde_json::from_str::<Meta>(&s).ok())
                    .ok_or(())
                    .map_err(|_| ())
                {
                    if m.expires <= now {
                        let stem = p.with_extension("blob");
                        let _ = std::fs::remove_file(&stem);
                        let _ = std::fs::remove_file(&p);
                        swept += 1;
                    }
                }
            }
        }
        swept
    }
}

async fn put_blob(
    axum::extract::State(srv): axum::extract::State<Arc<Srv>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> (axum::http::StatusCode, String) {
    use axum::http::StatusCode as S;
    if let Some(tok) = &srv.put_token {
        let ok = headers
            .get("x-put-token")
            .and_then(|v| v.to_str().ok())
            .map_or(false, |v| v == tok);
        if !ok {
            return (S::UNAUTHORIZED, "bad put token".into());
        }
    }
    if body.len() > srv.max_blob {
        srv.log("put", "-", "rejected: too large");
        return (S::PAYLOAD_TOO_LARGE, "blob too large".into());
    }
    let ttl = headers
        .get("x-ttl-secs")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(srv.default_ttl)
        .min(srv.max_ttl);
    let max_dl = headers
        .get("x-downloads")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(srv.default_downloads)
        .max(1);
    let hash = sha256_hex(&body);
    let mut tok_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut tok_bytes);
    let token = hex::encode(tok_bytes);
    let meta = Meta {
        expires: unix_now() + ttl,
        token_sha: sha256_hex(token.as_bytes()),
        max_downloads: max_dl,
        downloads: 0,
    };
    if std::fs::write(srv.blob_path(&hash), &body).is_err()
        || std::fs::write(
            srv.meta_path(&hash),
            serde_json::to_string(&meta).unwrap(),
        )
        .is_err()
    {
        srv.log("put", &hash, "failed: io");
        return (S::INTERNAL_SERVER_ERROR, "store failed".into());
    }
    srv.log("put", &hash, "stored");
    (
        S::OK,
        serde_json::json!({"sha256": hash, "token": token, "expires": meta.expires})
            .to_string(),
    )
}

async fn get_blob(
    axum::extract::State(srv): axum::extract::State<Arc<Srv>>,
    axum::extract::Path(hash): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> (axum::http::StatusCode, axum::body::Bytes) {
    use axum::http::StatusCode as S;
    let bad = |code: S, msg: &str| (code, axum::body::Bytes::from(msg.to_string()));
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) || hash.len() != 64 {
        return bad(S::BAD_REQUEST, "bad hash");
    }
    let meta_raw = match std::fs::read_to_string(srv.meta_path(&hash)) {
        Ok(s) => s,
        Err(_) => {
            srv.log("get", &hash, "miss");
            return bad(S::NOT_FOUND, "no such blob");
        }
    };
    let mut meta: Meta = match serde_json::from_str(&meta_raw) {
        Ok(m) => m,
        Err(_) => return bad(S::INTERNAL_SERVER_ERROR, "corrupt meta"),
    };
    if meta.expires <= unix_now() {
        let _ = std::fs::remove_file(srv.blob_path(&hash));
        let _ = std::fs::remove_file(srv.meta_path(&hash));
        srv.log("get", &hash, "expired");
        return bad(S::GONE, "expired");
    }
    let token = q.get("t").cloned().unwrap_or_default();
    if sha256_hex(token.as_bytes()) != meta.token_sha {
        srv.log("get", &hash, "bad token");
        return bad(S::FORBIDDEN, "bad token");
    }
    let body = match std::fs::read(srv.blob_path(&hash)) {
        Ok(b) => b,
        Err(_) => return bad(S::NOT_FOUND, "blob missing"),
    };
    meta.downloads += 1;
    if meta.downloads >= meta.max_downloads {
        // One-time redemption: first successful download consumes the link.
        let _ = std::fs::remove_file(srv.blob_path(&hash));
        let _ = std::fs::remove_file(srv.meta_path(&hash));
        srv.log("get", &hash, "redeemed+deleted");
    } else {
        let _ = std::fs::write(srv.meta_path(&hash), serde_json::to_string(&meta).unwrap());
        srv.log("get", &hash, "served");
    }
    (S::OK, axum::body::Bytes::from(body))
}

// ----------------------------------------------------------------- main ---

fn take(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn usage() -> ! {
    eprintln!(
        "jc-share — sealed blob exchange\n\
         serve      --dir <dir> --bind <addr> [--put-token T] [--access-log f]\n\
         seal       --to <ed25519-pubhex> --in <f> --out <f> [--kem-to <mlkem768-pubhex>]\n\
         unseal     --seed <hex|@file> --in <f> --out <f>\n\
         kem-keygen --seed <hex|@file>   (prints your ML-KEM-768 pubkey for the roster)\n\
         publish    --relay <url> --file <f> [--ttl-secs N] [--downloads N] [--put-token T]\n\
         fetch      <url> --out <f> [--unseal --seed <hex|@file>]\n\
         \n\
         seal with --kem-to produces the PQ-hybrid v2 envelope\n\
         (x25519-mlkem768-xchacha20poly1305-v2): breaking it requires\n\
         defeating BOTH X25519 and ML-KEM-768."
    );
    std::process::exit(2);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let rest = &args[2.min(args.len())..];
    match cmd {
        "serve" => {
            let dir = PathBuf::from(take(rest, "--dir").unwrap_or_else(|| "blobs".into()));
            std::fs::create_dir_all(&dir).expect("cannot create blob dir");
            let bind: SocketAddr = take(rest, "--bind")
                .unwrap_or_else(|| "127.0.0.1:8088".into())
                .parse()
                .expect("bad --bind");
            let srv = Arc::new(Srv {
                dir,
                put_token: take(rest, "--put-token"),
                access_log: PathBuf::from(
                    take(rest, "--access-log").unwrap_or_else(|| "access.log".into()),
                ),
                max_blob: take(rest, "--max-blob")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(256 * 1024 * 1024),
                max_ttl: take(rest, "--max-ttl")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(7 * 86400),
                default_ttl: take(rest, "--default-ttl")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(86400),
                default_downloads: take(rest, "--default-downloads")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1),
            });
            let sweeper = srv.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let n = sweeper.sweep();
                    if n > 0 {
                        sweeper.log("sweep", "-", &format!("{n} expired"));
                    }
                }
            });
            let app = axum::Router::new()
                .route("/blob", axum::routing::put(put_blob))
                .route("/blob/:hash", axum::routing::get(get_blob))
                .with_state(srv);
            eprintln!("jc-share serving on {bind}");
            let listener = tokio::net::TcpListener::bind(bind).await.expect("bind");
            axum::serve(listener, app).await.expect("serve");
        }
        "seal" => {
            let to = take(rest, "--to").expect("--to required");
            let kem_to = take(rest, "--kem-to");
            let inp = take(rest, "--in").expect("--in required");
            let out = take(rest, "--out").expect("--out required");
            let pt = std::fs::read(&inp).expect("cannot read --in");
            let enc = seal_inner(DOMAIN_SHARE, &to, kem_to.as_deref(), &pt);
            std::fs::write(&out, serde_json::to_string_pretty(&enc).unwrap()).unwrap();
            println!(
                "sealed {} -> {} ({} bytes, {})",
                inp,
                out,
                pt.len(),
                enc.scheme
            );
        }
        "kem-keygen" => {
            use ml_kem::EncodedSizeUser;
            let seed = load_seed(&take(rest, "--seed").expect("--seed required"));
            let (_dk, ek) = kem_keys_from_seed(&seed);
            println!("{}", hex::encode(ek.as_bytes()));
        }
        "unseal" => {
            let seed = load_seed(&take(rest, "--seed").expect("--seed required"));
            let inp = take(rest, "--in").expect("--in required");
            let out = take(rest, "--out").expect("--out required");
            let enc: EncField =
                serde_json::from_str(&std::fs::read_to_string(&inp).expect("cannot read --in"))
                    .expect("--in is not a sealed file");
            let pt = unseal_bytes(&enc, &seed);
            std::fs::write(&out, &pt).unwrap();
            println!("unsealed {} -> {} ({} bytes)", inp, out, pt.len());
        }
        "publish" => {
            let relay = take(rest, "--relay").expect("--relay required");
            let file = take(rest, "--file").expect("--file required");
            let body = std::fs::read(&file).expect("cannot read --file");
            let client = reqwest::Client::new();
            let mut req = client.put(format!("{}/blob", relay.trim_end_matches('/')));
            if let Some(t) = take(rest, "--put-token") {
                req = req.header("x-put-token", t);
            }
            if let Some(t) = take(rest, "--ttl-secs") {
                req = req.header("x-ttl-secs", t);
            }
            if let Some(d) = take(rest, "--downloads") {
                req = req.header("x-downloads", d);
            }
            let resp: serde_json::Value = serde_json::from_str(
                &req.body(body)
                    .send()
                    .await
                    .expect("publish failed")
                    .text()
                    .await
                    .expect("bad relay response"),
            )
            .expect("relay response is not JSON");
            let hash = resp["sha256"].as_str().unwrap();
            let token = resp["token"].as_str().unwrap();
            println!("sha256 {hash}");
            println!(
                "link   {}/blob/{}?t={}",
                relay.trim_end_matches('/'),
                hash,
                token
            );
        }
        "fetch" => {
            let url = rest.first().filter(|a| !a.starts_with("--")).expect("url required").clone();
            let hash = url
                .split("/blob/")
                .nth(1)
                .and_then(|s| s.split('?').next())
                .expect("url must contain /blob/<sha256>");
            let resp = reqwest::get(&url).await.expect("fetch failed");
            let status = resp.status();
            let body = resp.bytes().await.expect("body");
            if !status.is_success() {
                eprintln!("fetch failed: HTTP {} — {}", status, String::from_utf8_lossy(&body));
                std::process::exit(1);
            }
            if sha256_hex(&body) != hash {
                eprintln!("FATAL: fetched bytes do not match address hash {hash}");
                std::process::exit(1);
            }
            if rest.iter().any(|a| a == "--unseal") {
                let seed = load_seed(&take(rest, "--seed").expect("--seed required"));
                let enc: EncField = serde_json::from_slice(&body).expect("blob is not sealed");
                let pt = unseal_bytes(&enc, &seed);
                let out = take(rest, "--out").expect("--out required");
                std::fs::write(&out, &pt).unwrap();
                println!("fetched+unsealed -> {} ({} bytes)", out, pt.len());
            } else {
                let out = take(rest, "--out").expect("--out required");
                std::fs::write(&out, &body).unwrap();
                println!("fetched -> {} ({} bytes, sha256 verified)", out, body.len());
            }
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_unseal_roundtrip_share_domain() {
        // ed25519 pub = clamped_scalar * B in Edwards form; the clamped
        // scalar is exactly what x25519_scalar_from_ed_seed returns
        // (SHA-512(seed)[..32], clamped).
        use curve25519_dalek_ng::scalar::Scalar;
        let mut seed = [7u8; 32];
        seed[0] ^= 0x42;
        let s = Scalar::from_bytes_mod_order(x25519_scalar_from_ed_seed(&seed));
        let ed_pub = (&s * &curve25519_dalek_ng::constants::ED25519_BASEPOINT_TABLE)
            .compress()
            .0;
        let pt = b"hello share";
        let enc = seal_bytes(DOMAIN_SHARE, &hex::encode(ed_pub), pt);
        let back = unseal_bytes(&enc, &seed);
        assert_eq!(back, pt);
    }

    #[test]
    fn unseal_falls_back_to_dealing_domain() {
        let seed = [9u8; 32];
        let s = curve25519_dalek_ng::scalar::Scalar::from_bytes_mod_order(
            x25519_scalar_from_ed_seed(&seed),
        );
        let ed_pub = (&s * &curve25519_dalek_ng::constants::ED25519_BASEPOINT_TABLE)
            .compress()
            .0;
        let enc = seal_bytes(DOMAIN_DEALING, &hex::encode(ed_pub), b"dealing blob");
        let back = unseal_bytes(&enc, &seed);
        assert_eq!(back, b"dealing blob");
    }

    #[test]
    fn seal_v2_hybrid_roundtrip() {
        use curve25519_dalek_ng::scalar::Scalar;
        use ml_kem::EncodedSizeUser;
        let seed = [11u8; 32];
        let s = Scalar::from_bytes_mod_order(x25519_scalar_from_ed_seed(&seed));
        let ed_pub = (&s * &curve25519_dalek_ng::constants::ED25519_BASEPOINT_TABLE)
            .compress()
            .0;
        let (_dk, ek) = kem_keys_from_seed(&seed);
        let kem_pub = hex::encode(ek.as_bytes());
        let pt = b"dealing-priv for idx3";
        let enc = seal_inner(
            DOMAIN_DEALING,
            &hex::encode(ed_pub),
            Some(&kem_pub),
            pt,
        );
        assert_eq!(enc.scheme, SCHEME_V2);
        assert!(enc.kem_ct_hex.is_some());
        let back = unseal_bytes(&enc, &seed);
        assert_eq!(back, pt);
        // wrong seed must fail (different kem dk AND different x25519 scalar)
        assert!(std::panic::catch_unwind(|| unseal_bytes(&enc, &[12u8; 32])).is_err());
    }

    #[test]
    fn kem_keygen_is_deterministic() {
        use ml_kem::EncodedSizeUser;
        let seed = [5u8; 32];
        let (_d1, e1) = kem_keys_from_seed(&seed);
        let (_d2, e2) = kem_keys_from_seed(&seed);
        assert_eq!(e1.as_bytes(), e2.as_bytes());
    }

    #[test]
    fn unseal_wrong_seed_fails() {
        let seed = [1u8; 32];
        let s = curve25519_dalek_ng::scalar::Scalar::from_bytes_mod_order(
            x25519_scalar_from_ed_seed(&seed),
        );
        let ed_pub = (&s * &curve25519_dalek_ng::constants::ED25519_BASEPOINT_TABLE)
            .compress()
            .0;
        let enc = seal_bytes(DOMAIN_SHARE, &hex::encode(ed_pub), b"secret");
        let other_seed = [2u8; 32];
        assert!(std::panic::catch_unwind(|| unseal_bytes(&enc, &other_seed)).is_err());
    }
}
