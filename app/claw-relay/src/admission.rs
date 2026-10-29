//! On-chain cw4 admission for claw-relay.
//!
//! A background task polls a cw4-group contract's `list_members` smart
//! query over the chain REST API, maps member bech32 addresses to relay
//! npubs via a local JSON bindings file (re-read every refresh so ops
//! can update it without a restart), and swaps the active allowlist
//! under a shared RwLock.
//!
//! Semantics (docs/HOT-COLD-ATTESTATION.md):
//!   * fail CLOSED — until the first successful non-empty refresh the
//!     effective set is just the static break-glass entries, so any
//!     other pubkey is denied;
//!   * keep-last-good — on query/parse failure, or on a successful
//!     query that maps to zero npubs (a DAO with zero bound npubs is
//!     suspicious, not permissionless), the previous set is retained;
//!   * `--allowlist` entries are additive break-glass, merged into the
//!     active set on every refresh — never subtractive.
//!
//! The chain API may be a loopback plain-http endpoint or a public
//! https endpoint — operators point `--chain-api` wherever they can
//! reach committed state; TLS is provided by reqwest/rustls.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use reqwest::Client;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// The cw4 query we issue — every weighted member of the group.
const LIST_MEMBERS_QUERY: &str = r#"{"list_members":{}}"#;

/// Bound on a single smart-query round trip; a hung API must not stall
/// the refresh loop forever.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// Static configuration for the admission poller.
pub struct Cw4Config {
    /// e.g. "http://127.0.0.1:8082" or "https://status.junoclaw.xyz".
    pub api_base: String,
    /// bech32 address of the cw4-group contract.
    pub group_addr: String,
    /// `{"juno1...": "<64-hex-npub>", ...}` — re-read every refresh.
    pub bindings_path: Option<PathBuf>,
    /// Poll interval (`--cw4-refresh-secs`, default 60).
    pub refresh: Duration,
    /// `--allowlist` file entries — merged in on every refresh.
    pub breakglass: HashSet<String>,
}

/// Spawn point: refresh once, then sleep — forever.
pub async fn poll_loop(cfg: Cw4Config, target: Arc<RwLock<HashSet<String>>>) {
    let mut ever_populated = false;
    loop {
        poll_step(&cfg, &target, &mut ever_populated).await;
        tokio::time::sleep(cfg.refresh).await;
    }
}

/// One refresh iteration, split out of `poll_loop` so tests can drive
/// it deterministically. `ever_populated` distinguishes
/// "never populated (fail-closed)" from "populated then failed
/// (keep-last-good)" for logging.
async fn poll_step(
    cfg: &Cw4Config,
    target: &RwLock<HashSet<String>>,
    ever_populated: &mut bool,
) {
    match refresh_once(cfg).await {
        Ok(mapped) => match merge(&mapped, &cfg.breakglass) {
            Some(set) => {
                let n = set.len();
                *target.write().await = set;
                if !*ever_populated {
                    info!(npubs = n, "cw4 admission populated — writes open to bound members");
                }
                *ever_populated = true;
            }
            None => warn!(
                ever_populated = *ever_populated,
                "cw4 query ok but zero member npubs bound — keeping last-good set"
            ),
        },
        Err(e) => warn!(
            error = %e,
            ever_populated = *ever_populated,
            "cw4 admission refresh failed — keeping last-good set"
        ),
    }
}

/// Merge a cw4-derived npub set with break-glass entries.
/// Returns None when the mapped set is empty — the caller keeps the
/// previous allowlist rather than swapping in a break-glass-only set
/// that silently drops every member npub.
fn merge(mapped: &HashSet<String>, breakglass: &HashSet<String>) -> Option<HashSet<String>> {
    if mapped.is_empty() {
        return None;
    }
    let mut set = breakglass.clone();
    set.extend(mapped.iter().cloned());
    Some(set)
}

/// One full refresh: page through cw4 members, re-read the bindings
/// file from disk, map member addrs -> npubs.
async fn refresh_once(cfg: &Cw4Config) -> Result<HashSet<String>, String> {
    // list_members is paginated — unpaginated queries only return the
    // first page, silently locking out members beyond it as the DAO grows
    let mut members: Vec<String> = Vec::new();
    let mut start_after: Option<String> = None;
    loop {
        let query = match &start_after {
            Some(a) => format!(r#"{{"list_members":{{"start_after":"{a}","limit":30}}}}"#),
            None => LIST_MEMBERS_QUERY.to_string(),
        };
        let body = smart_query(&cfg.api_base, &cfg.group_addr, &query).await?;
        let page = parse_list_members(&body)?;
        let (raw_len, cursor) = page_cursor(&body);
        members.extend(page);
        if raw_len < 30 || cursor.is_none() {
            break;
        }
        start_after = cursor;
    }
    let bindings = load_bindings(cfg.bindings_path.as_deref())?;
    Ok(members
        .iter()
        .filter_map(|addr| bindings.get(addr).cloned())
        .collect())
}

/// (raw page length, last raw `addr`) — the pagination cursor reads the
/// RAW page, not the weight-filtered view: a zero-weight member at a
/// page boundary would otherwise skip the rest of the members.
fn page_cursor(body: &str) -> (usize, Option<String>) {
    let arr = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("data")?.get("members")?.as_array().cloned())
        .unwrap_or_default();
    let last = arr
        .last()
        .and_then(|m| m.get("addr")?.as_str().map(str::to_string));
    (arr.len(), last)
}

/// `{"addr": "<64-hex-npub>", ...}` — invalid npubs are dropped, a
/// missing/unreadable/non-object file is a hard refresh error.
fn load_bindings(path: Option<&Path>) -> Result<HashMap<String, String>, String> {
    let p = path.ok_or_else(|| "no --member-bindings configured".to_string())?;
    let s = std::fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
    parse_bindings(&s)
}

fn parse_bindings(s: &str) -> Result<HashMap<String, String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| format!("bindings json: {e}"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "bindings file must be a JSON object".to_string())?;
    let mut out = HashMap::new();
    for (addr, val) in obj {
        let Some(npub) = val.as_str() else { continue };
        let npub = npub.trim().to_lowercase();
        if npub.len() == 64 && npub.chars().all(|c| c.is_ascii_hexdigit()) {
            out.insert(addr.clone(), npub);
        }
    }
    Ok(out)
}

/// `{"data":{"members":[{"addr":"juno1...","weight":1},...]}}` →
/// addrs with weight > 0.
fn parse_list_members(body: &str) -> Result<Vec<String>, String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("smart query json: {e}"))?;
    let members = v
        .get("data")
        .and_then(|d| d.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| "response missing data.members".to_string())?;
    Ok(members
        .iter()
        .filter(|m| m.get("weight").and_then(|w| w.as_u64()).unwrap_or(0) > 0)
        .filter_map(|m| m.get("addr").and_then(|a| a.as_str()).map(str::to_string))
        .collect())
}

/// `GET <api>/api/wasm/smart/<contract>?query=<b64url-nopad(json)>`.
/// Both http:// (loopback) and https:// (public endpoints) are valid.
async fn smart_query(api_base: &str, contract: &str, query_json: &str) -> Result<String, String> {
    let url = smart_url(api_base, contract, query_json)?;
    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("http client build: {e}"))?;
    client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("get {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("http status: {e}"))?
        .text()
        .await
        .map_err(|e| format!("read body: {e}"))
}

/// "http[s]://host[:port][/prefix]" + the smart-query path.
fn smart_url(api_base: &str, contract: &str, query_json: &str) -> Result<String, String> {
    let base = api_base.trim_end_matches('/');
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(format!(
            "--chain-api must be http:// or https:// (got {api_base})"
        ));
    }
    if base.len() == "http://".len() || base.len() == "https://".len() {
        return Err("empty authority in --chain-api".to_string());
    }
    Ok(format!(
        "{base}/api/wasm/smart/{contract}?query={}",
        b64url_nopad(query_json.as_bytes())
    ))
}

/// RFC 4648 base64url, no padding — what `/api/wasm/smart` expects in
/// the `query` parameter.
fn b64url_nopad(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 3);
    for ch in data.chunks(3) {
        let n = ((ch[0] as u32) << 16)
            | ((*ch.get(1).unwrap_or(&0) as u32) << 8)
            | (*ch.get(2).unwrap_or(&0) as u32);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        if ch.len() > 1 {
            out.push(A[(n >> 6) as usize & 63] as char);
        }
        if ch.len() > 2 {
            out.push(A[n as usize & 63] as char);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn b64url_vectors() {
        assert_eq!(b64url_nopad(b""), "");
        assert_eq!(b64url_nopad(b"f"), "Zg");
        assert_eq!(b64url_nopad(b"fo"), "Zm8");
        assert_eq!(b64url_nopad(b"foo"), "Zm9v");
        assert_eq!(b64url_nopad(b"foob"), "Zm9vYg");
        assert_eq!(b64url_nopad(b"fooba"), "Zm9vYmE");
        assert_eq!(b64url_nopad(b"foobar"), "Zm9vYmFy");
        // the actual query the poller sends
        assert_eq!(
            b64url_nopad(LIST_MEMBERS_QUERY.as_bytes()),
            "eyJsaXN0X21lbWJlcnMiOnt9fQ"
        );
        // url-safe alphabet: '+'/'/' would break the query param
        assert!(!b64url_nopad(b"\xff\xff\xff").contains('+'));
        assert_eq!(b64url_nopad(b"\xff\xff\xff"), "____");
    }

    #[test]
    fn parse_members_filters_zero_weight() {
        let body = r#"{"data":{"members":[
            {"addr":"juno1aaa","weight":1},
            {"addr":"juno1bbb","weight":0},
            {"addr":"juno1ccc","weight":7}
        ]}}"#;
        let got = parse_list_members(body).unwrap();
        assert_eq!(got, vec!["juno1aaa".to_string(), "juno1ccc".to_string()]);
    }

    #[test]
    fn parse_members_empty_and_bad() {
        // zero members parses fine — the EMPTY result is handled by
        // merge() as keep-last-good, not by the parser
        assert!(parse_list_members(r#"{"data":{"members":[]}}"#)
            .unwrap()
            .is_empty());
        assert!(parse_list_members(r#"{"data":{}}"#).is_err());
        assert!(parse_list_members("not json").is_err());
        assert!(parse_list_members(r#"{"data":{"members":[{"weight":1}]}}"#)
            .unwrap()
            .is_empty()); // member without addr contributes nothing
    }

    #[test]
    fn parse_bindings_maps_valid_npubs() {
        let npub = "a".repeat(64);
        let s = format!(
            r#"{{"juno1aaa":"{npub}","juno1bbb":"not-hex","juno1ccc":"{}"}}"#,
            "B".repeat(64)
        );
        let m = parse_bindings(&s).unwrap();
        assert_eq!(m.get("juno1aaa").unwrap(), &npub);
        // uppercase hex normalized to lowercase
        assert_eq!(m.get("juno1ccc").unwrap(), &npub.replace('a', "b"));
        // invalid npub dropped
        assert!(!m.contains_key("juno1bbb"));
        assert!(parse_bindings("[1,2]").is_err());
        assert!(parse_bindings("garbage").is_err());
    }

    #[test]
    fn merge_semantics() {
        let breakglass: HashSet<String> = ["op".repeat(32)].iter().cloned().collect();
        let mapped: HashSet<String> = ["aa".repeat(32), "bb".repeat(32)].iter().cloned().collect();
        // empty mapped -> None: keep last good, never install breakglass-only
        assert!(merge(&HashSet::new(), &breakglass).is_none());
        let merged = merge(&mapped, &breakglass).unwrap();
        assert_eq!(merged.len(), 3); // 2 members + 1 breakglass, additive
        assert!(merged.contains(&"aa".repeat(32)));
        assert!(merged.contains(&"op".repeat(32)));
    }

    #[test]
    fn smart_url_joining() {
        assert_eq!(
            smart_url("http://127.0.0.1:8082", "juno1c", "{}").unwrap(),
            "http://127.0.0.1:8082/api/wasm/smart/juno1c?query=e30"
        );
        // https endpoints are valid for external operators
        assert_eq!(
            smart_url("https://status.junoclaw.xyz/", "juno1c", "{}").unwrap(),
            "https://status.junoclaw.xyz/api/wasm/smart/juno1c?query=e30"
        );
        // prefix paths survive the join
        assert_eq!(
            smart_url("http://h:1/prefix", "juno1c", "{}").unwrap(),
            "http://h:1/prefix/api/wasm/smart/juno1c?query=e30"
        );
        assert!(smart_url("status.junoclaw.xyz", "c", "{}").is_err());
        assert!(smart_url("http://", "c", "{}").is_err());
    }

    /// Serve canned HTTP responses from a local listener; each accept
    /// pops the next canned response in order.
    async fn canned_server(resps: Vec<&'static str>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for r in resps {
                if let Ok((mut s, _)) = listener.accept().await {
                    let mut buf = vec![0u8; 4096];
                    let _ = s.read(&mut buf).await;
                    let _ = s.write_all(r.as_bytes()).await;
                }
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn refresh_populates_then_keeps_last_good() {
        let dir = std::env::temp_dir().join(format!("claw-adm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let npub = "c".repeat(64);
        let bindings = dir.join("bindings.json");
        std::fs::write(&bindings, format!(r#"{{"juno1aaa":"{npub}"}}"#)).unwrap();

        let good = "HTTP/1.1 200 OK\r\nContent-Length: 84\r\n\r\n{\"data\":{\"members\":[{\"addr\":\"juno1aaa\",\"weight\":1},{\"addr\":\"juno1zzz\",\"weight\":1}]}}";
        let bad = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        let api = canned_server(vec![good, bad]).await;

        let cfg = Cw4Config {
            api_base: api,
            group_addr: "juno1contract".into(),
            bindings_path: Some(bindings),
            refresh: Duration::from_secs(60),
            breakglass: HashSet::new(),
        };
        let target = RwLock::new(HashSet::new());
        let mut ever = false;

        // first refresh: member juno1aaa maps to npub; juno1zzz unbound
        poll_step(&cfg, &target, &mut ever).await;
        assert!(ever);
        assert_eq!(target.read().await.len(), 1);
        assert!(target.read().await.contains(&npub));

        // second refresh: HTTP 500 -> last-good retained
        poll_step(&cfg, &target, &mut ever).await;
        assert!(target.read().await.contains(&npub));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn refresh_zero_mapped_keeps_closed() {
        let dir = std::env::temp_dir().join(format!("claw-adm0-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bindings = dir.join("bindings.json");
        std::fs::write(&bindings, r#"{"juno1nobody":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"}"#).unwrap();

        let good = "HTTP/1.1 200 OK\r\nContent-Length: 53\r\n\r\n{\"data\":{\"members\":[{\"addr\":\"juno1aaa\",\"weight\":1}]}}";
        let api = canned_server(vec![good]).await;

        let cfg = Cw4Config {
            api_base: api,
            group_addr: "juno1contract".into(),
            bindings_path: Some(bindings),
            refresh: Duration::from_secs(60),
            breakglass: HashSet::new(),
        };
        let target = RwLock::new(HashSet::new());
        let mut ever = false;
        // query succeeds but zero npubs bound -> suspicious, keep the
        // (still empty) last-good set; never populated -> fail closed
        poll_step(&cfg, &target, &mut ever).await;
        assert!(!ever);
        assert!(target.read().await.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
