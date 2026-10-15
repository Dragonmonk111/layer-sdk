//! jc-explorer — thin public explorer for JunoClaw.
//!
//! Server-rendered: polls `layer.lightclient.v1.Query` for finalized blocks,
//! decodes the payload + hybrid certificate envelope, and serves HTML + JSON
//! from one binary. Put it behind the Caddy edge on OVH.
//!
//!   jc-explorer --grpc 127.0.0.1:9090 --listen 127.0.0.1:8081 \
//!       --archive /path/to/archive.sqlite3
//!
//! With `--archive`, the CDC archiver's `blocks` table provides deep
//! history: /history pages back over every certified height, and /block/{h}
//! falls back to archive rows when the node's lightclient window has pruned
//! them. Archive rows carry seq/height/state_root/proposer/cert — no tx
//! payloads, so archive-sourced views are marked as such.
//!
//! Pages:  /            status + recent blocks (auto-refresh)
//!         /block/{h}   block detail + hybrid cert panel (live or archive)
//!         /history     deep history from the archive (archive mode only)
//!         /address/{a} account page (balances/txs once traffic exists)
//! JSON:   /api/status  /api/blocks?n=  /api/block/{h}
//!         /api/archive/status  /api/archive/blocks?to=&n=
//!         /api/address/{a}

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use axum::routing::get;
use axum::Router;
use layer_proto::cosmos::base::abci::v1beta1::TxResponse;
use layer_proto::layer::lightclient::v1::query_client::QueryClient as LcClient;
use layer_proto::layer::lightclient::v1::{
    QueryBlockRequest, QueryLatestHeightRequest,
};
use prost::Message as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tonic::transport::Channel;

// ---------------------------------------------------------------------------
// Decoded block model
// ---------------------------------------------------------------------------

/// Wire-mirror of app/slay3rd/src/block.rs `BlockPayload` (bincode field order).
#[derive(Deserialize)]
struct BlockPayload {
    height: u64,
    timestamp_nanos: u64,
    proposer: Vec<u8>,
    txs: Vec<Vec<u8>>,
    parent_digest: [u8; 32],
    state_root: [u8; 32],
}

/// Hybrid certificate envelope (tools/verify-cert parser):
/// `[0x01][classical BLS cert: 48 B][signers BitMap][MAYO2 sig: 186 B × k]`
struct HybridEnvelope {
    participants: usize,
    signers: Vec<usize>,
    mayo_sig_count: usize,
}

fn parse_hybrid_envelope(bytes: &[u8]) -> Result<HybridEnvelope, String> {
    const MAYO2_SIG_BYTES: usize = 186;
    const MIN_LEN: usize = 1 + 48 + 8 + 1;
    if bytes.len() < MIN_LEN {
        return Err("not a hybrid certificate".into());
    }
    if bytes[0] != 0x01 {
        return Err("not a hybrid certificate".into());
    }
    let participants = u64::from_be_bytes(bytes[49..57].try_into().unwrap()) as usize;
    if participants == 0 || participants > 4096 {
        return Err("not a hybrid certificate".into());
    }
    let bitmap_bytes = participants.div_ceil(8);
    if bytes.len() < 57 + bitmap_bytes {
        return Err("not a hybrid certificate".into());
    }
    let bitmap = &bytes[57..57 + bitmap_bytes];
    let signers: Vec<usize> = (0..participants)
        .filter(|i| (bitmap[i / 8] >> (i % 8)) & 1 == 1)
        .collect();
    let rest = &bytes[57 + bitmap_bytes..];
    if rest.len() != signers.len() * MAYO2_SIG_BYTES {
        return Err("not a hybrid certificate".into());
    }
    let mayo_sig_count = signers.len();
    Ok(HybridEnvelope {
        participants,
        signers,
        mayo_sig_count,
    })
}

#[derive(Clone, Serialize)]
struct BlockInfo {
    height: u64,
    timestamp_nanos: u64,
    digest: String,
    tx_count: usize,
    tx_hashes: Vec<String>,
    state_root: String,
    proposer: String,
    finalized: bool,
    hybrid_cert: bool,
    signers: Vec<usize>,
    participants: usize,
    mayo_sigs: usize,
    cert_bytes: usize,
}

struct Cache {
    tip: u64,
    tip_time: Option<Instant>,
    blocks: HashMap<u64, BlockInfo>,
}

/// Archive row from the CDC `blocks` table — certified block metadata.
/// The archive carries no payload bytes, so tx_count/digest stay honest:
/// digest is absent (not stored), state_root/app_hash come from the node.
#[derive(Clone, Serialize)]
struct ArchiveBlock {
    seq: u64,
    height: u64,
    app_hash: Option<String>,
    state_root: Option<String>,
    proposer: Option<String>,
    cert_bytes: usize,
    finalized: bool,
    hybrid_cert: bool,
    signers: Vec<usize>,
    participants: usize,
    mayo_sigs: usize,
}

/// Shared state: the live cache + an optional read-only archive connection.
struct Shared2 {
    grpc: String,
    cache: RwLock<Cache>,
    archive: Option<std::sync::Mutex<rusqlite::Connection>>,
}

type AppState = Arc<Shared2>;

fn archive_lookup(conn: &rusqlite::Connection, height: u64) -> Option<ArchiveBlock> {
    conn.query_row(
        "SELECT seq, height, app_hash, state_root, proposer, cert FROM blocks WHERE height=?1",
        [height as i64],
        |r| {
            let cert: Option<Vec<u8>> = r.get(5)?;
            let cert = cert.unwrap_or_default();
            let (hybrid, signers, participants, mayo) = match parse_hybrid_envelope(&cert) {
                Ok(e) => (true, e.signers, e.participants, e.mayo_sig_count),
                Err(_) => (false, vec![], 0, 0),
            };
            Ok(ArchiveBlock {
                seq: r.get::<_, i64>(0)? as u64,
                height: r.get::<_, i64>(1)? as u64,
                app_hash: r.get::<_, Option<Vec<u8>>>(2)?.map(|b| hex::encode(b)),
                state_root: r.get::<_, Option<Vec<u8>>>(3)?.map(|b| hex::encode(b)),
                proposer: r.get::<_, Option<Vec<u8>>>(4)?.map(|b| hex::encode(b)),
                cert_bytes: cert.len(),
                finalized: !cert.is_empty(),
                hybrid_cert: hybrid,
                signers,
                participants,
                mayo_sigs: mayo,
            })
        },
    )
    .ok()
}

fn decode_block(height: u64, ts: u64, cert: &[u8], payload: &[u8]) -> BlockInfo {
    let (digest, tx_count, state_root, proposer, tx_hashes) = match BlockPayload::from_bytes_ext(payload) {
        Some(p) => (
            hex::encode(Sha256::digest(payload)),
            p.txs.len(),
            hex::encode(p.state_root),
            hex::encode(&p.proposer[..p.proposer.len().min(8)]),
            // CometBFT convention: txhash = sha256(raw tx), upper-hex.
            p.txs
                .iter()
                .map(|t| hex::encode_upper(Sha256::digest(t)))
                .collect::<Vec<_>>(),
        ),
        None => ("—".into(), 0, "—".into(), "—".into(), vec![]),
    };
    let (hybrid, signers, participants, mayo) = match parse_hybrid_envelope(cert) {
        Ok(e) => (true, e.signers, e.participants, e.mayo_sig_count),
        Err(_) => (false, vec![], 0, 0),
    };
    BlockInfo {
        height,
        timestamp_nanos: ts,
        digest,
        tx_count,
        tx_hashes,
        state_root,
        proposer,
        finalized: !cert.is_empty(),
        hybrid_cert: hybrid,
        signers,
        participants,
        mayo_sigs: mayo,
        cert_bytes: cert.len(),
    }
}

impl BlockPayload {
    fn from_bytes_ext(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        bincode::deserialize(data).ok()
    }
}

// ---------------------------------------------------------------------------
// gRPC fetch layer
// ---------------------------------------------------------------------------

async fn lc_client(grpc: &str) -> Result<LcClient<Channel>, tonic::transport::Error> {
    let uri = if grpc.contains("://") {
        grpc.to_string()
    } else {
        format!("http://{grpc}")
    };
    Channel::from_shared(uri).unwrap().connect().await.map(LcClient::new)
}

async fn fetch_block(grpc: &str, h: u64) -> Option<BlockInfo> {
    let mut c = lc_client(grpc).await.ok()?;
    let r = c
        .block(QueryBlockRequest { height: h })
        .await
        .ok()?
        .into_inner();
    Some(decode_block(
        r.height,
        r.timestamp_nanos,
        &r.certificate_bytes,
        &r.payload_bytes,
    ))
}

// ---------------------------------------------------------------------------
// HTML rendering — hand-rolled, monospace, honest thin wedge
// ---------------------------------------------------------------------------

const CSS: &str = r#"
<style>
  :root{color-scheme:dark}
  body{background:#0b0e14;color:#d7dde8;font:14px/1.5 ui-monospace,Menlo,Consolas,monospace;
       max-width:1060px;margin:0 auto;padding:24px}
  a{color:#7dd3fc;text-decoration:none} a:hover{text-decoration:underline}
  h1{font-size:20px;margin:0 0 4px} h2{font-size:15px;color:#8b96a8;font-weight:600;margin:28px 0 10px}
  .sub{color:#8b96a8;font-size:12.5px}
  .badge{display:inline-block;padding:1px 8px;border-radius:10px;font-size:11px;font-weight:700}
  .ok{background:#14532d;color:#86efac} .hyb{background:#1e3a5f;color:#7dd3fc}
  .warn{background:#5f2e1e;color:#fca5a5}
  table{border-collapse:collapse;width:100%} td,th{padding:5px 10px;text-align:left;
       border-bottom:1px solid #1e2530;font-size:12.5px} th{color:#8b96a8;font-weight:600}
  .card{background:#11151d;border:1px solid #1e2530;border-radius:8px;padding:14px 18px;margin:14px 0}
  .kv{display:grid;grid-template-columns:150px 1fr;gap:3px 14px;font-size:12.5px}
  .kv div:nth-child(odd){color:#8b96a8}
  .hash{word-break:break-all;font-size:11.5px}
  .pillbar{display:flex;gap:18px;flex-wrap:wrap;margin:16px 0}
  .pill{background:#11151d;border:1px solid #1e2530;border-radius:8px;padding:10px 16px}
  .pill b{display:block;font-size:18px} .pill span{color:#8b96a8;font-size:11px}
</style>
"#;

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>{title} — JunoClaw Explorer</title>{CSS}</head><body>{body}</body></html>"
    ))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn ago(nanos: u64) -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64 - nanos as i64)
        .map(|d| (d / 1_000_000_000).max(0))
        .unwrap_or(0);
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn home(State(st): State<AppState>) -> Html<String> {
    let (tip, blocks) = {
        let c = st.cache.read().unwrap();
        let mut v: Vec<BlockInfo> = c.blocks.values().cloned().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.height));
        (c.tip, v)
    };
    let archive_pill = st.archive.as_ref().map(|a| {
        let conn = a.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))
            .unwrap_or(0);
        format!("<div class=pill><b>{n}</b><span>archived blocks</span></div>")
    }).unwrap_or_default();
    let history_link = if st.archive.is_some() {
        " · <a href='/history'>deep history</a>"
    } else {
        ""
    };

    let mut rows = String::new();
    for b in blocks.iter().take(40) {
        let cert = if b.hybrid_cert {
            format!(
                "<span class='badge hyb'>HYBRID</span> {}/{} signers",
                b.signers.len(),
                b.participants
            )
        } else if b.finalized {
            "<span class='badge ok'>BLS</span>".into()
        } else {
            "<span class='badge warn'>PENDING</span>".into()
        };
        rows += &format!(
            "<tr><td><a href='/block/{}'>{}</a></td><td>{}</td>\
             <td class=hash>{}…</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            b.height,
            b.height,
            ago(b.timestamp_nanos),
            esc(&b.digest[..b.digest.len().min(16)]),
            b.tx_count,
            cert,
            b.cert_bytes
        );
    }

    page(
        "JunoClaw",
        &format!(
            "<h1>JunoClaw <span class=hyb badge>HYBRID PQ</span></h1>\
             <div class=sub>live from the chain's own light-client API — every block below carries a BLS12-381 + MAYO2 certificate you can independently verify</div>\
             <div class=pillbar>\
               <div class=pill><b>{tip}</b><span>finalized height</span></div>\
               <div class=pill><b>{}</b><span>blocks cached</span></div>{archive_pill}\
               <div class=pill><b>BLS+MAYO2</b><span>certificate scheme</span></div>\
               <div class=pill><b>deterministic</b><span>CosmWasm execution</span></div>\
             </div>\
             <form action='/search' method='get' style='margin:6px 0 14px'>\
             <input name='q' placeholder='block height or juno1… address' style='width:420px;background:#0b0e14;color:#d7dde8;border:1px solid #1e2530;border-radius:6px;padding:7px 10px;font:inherit'>\
             <button style='background:#1e3a5f;color:#7dd3fc;border:0;border-radius:6px;padding:7px 14px;font:inherit;cursor:pointer'>go</button></form>\
             <h2>Recent finalized blocks</h2>\
             <table><tr><th>Height</th><th>Age</th><th>Digest</th><th>Txs</th><th>Certificate</th><th>Cert bytes</th></tr>{rows}</table>\
             <div class=sub style='margin-top:22px'>auto-refresh 10s · thin community explorer{history_link} — raw cert bytes at /api/block/{{h}} verify offline with tools/verify-cert</div>\
             <script>setTimeout(()=>location.reload(),10000)</script>",
            blocks.len()
        ),
    )
}

async fn block_page(
    State(st): State<AppState>,
    Path(h): Path<u64>,
) -> Result<Html<String>, StatusCode> {
    // Archive fallback: heights the node's lightclient window has pruned
    // still render from the certified archive row.
    let b = {
        let hit = st.cache.read().unwrap().blocks.get(&h).cloned();
        match hit {
            Some(b) => Some(b),
            None => match fetch_block(&st.grpc, h).await {
                Some(b) => {
                    st.cache.write().unwrap().blocks.insert(h, b.clone());
                    Some(b)
                }
                None => None,
            },
        }
    };
    let b = match b {
        Some(b) => b,
        None => return archive_block_page(&st, h).await,
    };

    let cert_html = if b.hybrid_cert {
        format!(
            "<div class=kv>\
             <div>scheme</div><div><span class='badge hyb'>HYBRID</span> BLS12-381 aggregate + MAYO2 per-signer</div>\
             <div>participants</div><div>{}</div>\
             <div>signers</div><div>{} ({:?})</div>\
             <div>MAYO2 sigs</div><div>{} × 186 B</div>\
             <div>cert size</div><div>{} bytes</div>\
             </div>",
            b.participants,
            b.signers.len(),
            b.signers,
            b.mayo_sigs,
            b.cert_bytes
        )
    } else {
        "<div class=sub>classical BLS certificate or not yet finalized</div>".into()
    };

    let txs_html = if b.tx_hashes.is_empty() {
        "<div class=sub>no transactions</div>".to_string()
    } else {
        let rows: String = b
            .tx_hashes
            .iter()
            .map(|t| format!("<tr><td class=hash><a href='/tx/{t}'>{t}</a></td></tr>"))
            .collect();
        format!("<table><tr><th>tx hash</th></tr>{rows}</table>")
    };

    Ok(page(
        &format!("Block {}", b.height),
        &format!(
            "<h1><a href='/'>←</a> Block {} <span class='badge {}'>{}</span></h1>\
             <div class=card><div class=kv>\
             <div>timestamp</div><div>{} ({})</div>\
             <div>digest</div><div class=hash>{}</div>\
             <div>state root</div><div class=hash>{}</div>\
             <div>proposer</div><div class=hash>{}</div>\
             <div>transactions</div><div>{}</div>\
             </div></div>\
             <h2>Transactions</h2><div class=card>{txs_html}</div>\
             <h2>Finality certificate</h2><div class=card>{cert_html}</div>\
             <div class=sub>raw: <a href='/api/block/{h}'>/api/block/{h}</a> — independently verify with tools/verify-cert</div>",
            b.height,
            if b.finalized { "ok" } else { "warn" },
            if b.finalized { "FINALIZED" } else { "PENDING" },
            b.timestamp_nanos,
            ago(b.timestamp_nanos),
            esc(&b.digest),
            esc(&b.state_root),
            esc(&b.proposer),
            b.tx_count,
            h = h
        ),
    ))
}

#[derive(Deserialize)]
struct BlocksQ {
    n: Option<usize>,
}

/// Block detail rendered from the archive (live lightclient window missed).
async fn archive_block_page(st: &AppState, h: u64) -> Result<Html<String>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let ab = {
        let conn = arc.lock().unwrap();
        archive_lookup(&conn, h).ok_or(StatusCode::NOT_FOUND)?
    };

    let cert_html = if ab.hybrid_cert {
        format!(
            "<div class=kv>\
             <div>scheme</div><div><span class='badge hyb'>HYBRID</span> BLS12-381 aggregate + MAYO2 per-signer</div>\
             <div>participants</div><div>{}</div>\
             <div>signers</div><div>{} ({:?})</div>\
             <div>MAYO2 sigs</div><div>{} × 186 B</div>\
             <div>cert size</div><div>{} bytes</div>\
             </div>",
            ab.participants,
            ab.signers.len(),
            ab.signers,
            ab.mayo_sigs,
            ab.cert_bytes
        )
    } else {
        "<div class=sub>certificate pruned from the node's rolling window — height exists in archive</div>".into()
    };

    Ok(page(
        &format!("Block {h}"),
        &format!(
            "<h1><a href='/'>←</a> Block {} <span class='badge {}'>{}</span> <span class='badge hyb'>ARCHIVE</span></h1>\
             <div class=card><div class=kv>\
             <div>archive seq</div><div>{}</div>\
             <div>state root</div><div class=hash>{}</div>\
             <div>app hash</div><div class=hash>{}</div>\
             <div>proposer</div><div class=hash>{}</div>\
             </div></div>\
             <h2>Finality certificate</h2><div class=card>{cert_html}</div>\
             <div class=sub>served from the CDC archive — payload bytes are not stored; \
             raw row at <a href='/api/block/{h}'>/api/block/{h}</a></div>",
            ab.height,
            if ab.finalized { "ok" } else { "warn" },
            if ab.finalized { "FINALIZED" } else { "UNCERTIFIED" },
            ab.seq,
            ab.state_root.as_deref().unwrap_or("—"),
            ab.app_hash.as_deref().unwrap_or("—"),
            ab.proposer.as_deref().unwrap_or("—"),
            h = h
        ),
    ))
}

// ---------------------------------------------------------------------------
// /history — deep certified history from the archive
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct HistoryQ {
    to: Option<u64>,
}

async fn history_page(
    State(st): State<AppState>,
    Query(q): Query<HistoryQ>,
) -> Result<Html<String>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    const PAGE: i64 = 100;
    let (rows_html, tip, oldest) = {
        let conn = arc.lock().unwrap();
        let tip: i64 = conn
            .query_row("SELECT max(height) FROM blocks", [], |r| r.get(0))
            .unwrap_or(0);
        let to = q.to.map(|t| t.min(tip as u64)).unwrap_or(tip as u64) as i64;
        let mut stq = conn
            .prepare("SELECT seq, height, length(cert) FROM blocks WHERE height<=?1 ORDER BY height DESC LIMIT ?2")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let rows = stq
            .query_map([to, PAGE], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            })
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut s = String::new();
        let mut oldest = to;
        for r in rows.flatten() {
            oldest = r.1;
            s += &format!(
                "<tr><td><a href='/block/{0}'>{0}</a></td><td>{1}</td><td>{2}</td></tr>",
                r.1, r.0, r.2
            );
        }
        (s, tip, oldest)
    };

    let next = if oldest > 1 {
        format!("<a href='/history?to={}'>older ↓</a>", oldest - 1)
    } else {
        String::new()
    };
    let newer = if q.to.map(|t| t < tip as u64).unwrap_or(false) {
        format!("<a href='/history?to={}'>↑ newer</a> ", q.to.unwrap() + PAGE as u64)
    } else {
        String::new()
    };

    Ok(page(
        "History",
        &format!(
            "<h1><a href='/'>←</a> Certified history</h1>\
             <div class=sub>every row is a finalized height from the CDC archive — open one for its hybrid certificate + state root</div>\
             <table><tr><th>Height</th><th>Archive seq</th><th>Cert bytes</th></tr>{rows_html}</table>\
             <div class=sub style='margin-top:14px'>{newer}{next}</div>",
        ),
    ))
}

// ---------------------------------------------------------------------------
// /search + /address — block heights jump to /block; juno1… goes to /address
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct SearchQ {
    q: Option<String>,
}

async fn search(Query(q): Query<SearchQ>) -> axum::response::Redirect {
    let q = q.q.unwrap_or_default().trim().to_string();
    if let Ok(h) = q.parse::<u64>() {
        return axum::response::Redirect::to(&format!("/block/{h}"));
    }
    // 64-char hex = sha256 txhash (CometBFT convention).
    if q.len() == 64 && q.bytes().all(|b| b.is_ascii_hexdigit()) {
        return axum::response::Redirect::to(&format!("/tx/{}", q.to_ascii_uppercase()));
    }
    if q.starts_with("juno1") || q.starts_with("osmo1") {
        return axum::response::Redirect::to(&format!("/address/{q}"));
    }
    axum::response::Redirect::to("/")
}

async fn address_page(
    State(st): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Html<String>, StatusCode> {
    // Balances live in the archive's typed tables once the chain sees
    // bank traffic; until then the page is honestly empty.
    let balances_html = match &st.archive {
        Some(arc) => {
            let conn = arc.lock().unwrap();
            let mut stq = conn
                .prepare("SELECT denom, amount FROM balances WHERE address=?1")
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let rows = stq
                .query_map([&addr], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut s = String::new();
            let mut any = false;
            for r in rows.flatten() {
                any = true;
                s += &format!("<tr><td>{}</td><td>{}</td></tr>", esc(&r.0), esc(&r.1));
            }
            if any {
                format!("<table><tr><th>Denom</th><th>Amount</th></tr>{s}</table>")
            } else {
                "<div class=sub>no balances recorded — this address has not received a finalized bank send yet</div>".into()
            }
        }
        None => "<div class=sub>archive not attached — balances unavailable</div>".into(),
    };

    // Recent txs touching this address (from the archive's tx_addrs index).
    let txs_html = match &st.archive {
        Some(arc) => {
            let conn = arc.lock().unwrap();
            let mut stq = conn
                .prepare(
                    "SELECT t.txhash, t.height, t.code FROM tx_addrs a \
                     JOIN txs t ON t.txhash = a.txhash \
                     WHERE a.address=?1 ORDER BY t.seq DESC LIMIT 50",
                )
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let rows = stq
                .query_map([&addr], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let mut s = String::new();
            let mut any = false;
            for r in rows.flatten() {
                any = true;
                let badge = if r.2 == 0 {
                    "<span class='badge ok'>OK</span>"
                } else {
                    "<span class='badge warn'>FAIL</span>"
                };
                s += &format!(
                    "<tr><td class=hash><a href='/tx/{}'>{}</a></td><td>{}</td><td>{}</td></tr>",
                    esc(&r.0),
                    esc(&r.0),
                    r.1.map(|h| h.to_string()).unwrap_or("—".into()),
                    badge
                );
            }
            if any {
                format!("<table><tr><th>tx hash</th><th>height</th><th>result</th></tr>{s}</table>")
            } else {
                "<div class=sub>no indexed transactions touch this address yet</div>".into()
            }
        }
        None => "<div class=sub>archive not attached — tx history unavailable</div>".into(),
    };

    Ok(page(
        &format!("Address"),
        &format!(
            "<h1><a href='/'>←</a> Address</h1>\
             <div class=card><div class=kv><div>address</div><div class=hash>{}</div></div></div>\
             <h2>Balances</h2><div class=card>{balances_html}</div>\
             <h2>Transactions</h2><div class=card>{txs_html}</div>",
            esc(&addr)
        ),
    ))
}

/// Transaction detail from the archive's `txs` table.
async fn tx_page(
    State(st): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let row = {
        let conn = arc.lock().unwrap();
        conn.query_row(
            "SELECT txhash, seq, height, code, raw_log, gas_wanted, gas_used, response \
             FROM txs WHERE txhash=?1",
            [&hash],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<Vec<u8>>>(7)?,
                ))
            },
        )
        .ok()
    };
    let Some((txhash, _seq, height, code, raw_log, gas_wanted, gas_used, resp_bytes)) = row else {
        return Ok(page(
            "Tx",
            &format!(
                "<h1><a href='/'>←</a> Transaction</h1>\
                 <div class=card><div class=kv><div>hash</div><div class=hash>{}</div></div></div>\
                 <div class=sub>not in the archive index — the tx may be pruned, \
                 pending, or the hash is wrong</div>",
                esc(&hash)
            ),
        ));
    };

    let (badge, status) = if code == 0 {
        ("ok", "SUCCESS")
    } else {
        ("warn", "FAILED")
    };

    // Decode the stored TxResponse for the event log.
    let events_html = resp_bytes
        .and_then(|b| TxResponse::decode(b.as_slice()).ok())
        .map(|r| {
            let mut s = String::new();
            for ev in &r.events {
                s += &format!("<tr><td colspan=2><b>{}</b></td></tr>", esc(&ev.r#type));
                for a in &ev.attributes {
                    s += &format!(
                        "<tr><td class=sub>{}</td><td class=hash>{}</td></tr>",
                        esc(&a.key),
                        esc(&a.value)
                    );
                }
            }
            if s.is_empty() {
                "<div class=sub>no events</div>".into()
            } else {
                format!("<table>{s}</table>")
            }
        })
        .unwrap_or_else(|| "<div class=sub>response bytes undecodable</div>".into());

    Ok(page(
        "Transaction",
        &format!(
            "<h1><a href='/'>←</a> Transaction <span class='badge {badge}'>{status}</span></h1>\
             <div class=card><div class=kv>\
             <div>hash</div><div class=hash>{}</div>\
             <div>height</div><div>{}</div>\
             <div>code</div><div>{}</div>\
             <div>gas</div><div>{} used / {} wanted</div>\
             <div>log</div><div class=hash>{}</div>\
             </div></div>\
             <h2>Events</h2><div class=card>{events_html}</div>\
             <div class=sub>raw: <a href='/api/tx/{}'>/api/tx/{}</a></div>",
            esc(&txhash),
            height.map(|h| h.to_string()).unwrap_or("—".into()),
            code,
            gas_used,
            gas_wanted,
            esc(&raw_log),
            esc(&txhash),
            esc(&txhash),
        ),
    ))
}

async fn api_status(State(st): State<AppState>) -> Json<serde_json::Value> {
    let c = st.cache.read().unwrap();
    let archived = st.archive.as_ref().map(|a| {
        a.lock()
            .unwrap()
            .query_row("SELECT count(*) FROM blocks", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
    });
    Json(serde_json::json!({
        "tip": c.tip,
        "cached_blocks": c.blocks.len(),
        "archived_blocks": archived,
    }))
}

async fn api_blocks(
    State(st): State<AppState>,
    Query(q): Query<BlocksQ>,
) -> Json<serde_json::Value> {
    let c = st.cache.read().unwrap();
    let mut v: Vec<&BlockInfo> = c.blocks.values().collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.height));
    let n = q.n.unwrap_or(40).min(200);
    Json(serde_json::json!(v.iter().take(n).collect::<Vec<_>>()))
}

async fn api_block(
    State(st): State<AppState>,
    Path(h): Path<u64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(b) = st.cache.read().unwrap().blocks.get(&h) {
        return Ok(Json(serde_json::json!(b)));
    }
    if let Some(b) = fetch_block(&st.grpc, h).await {
        st.cache.write().unwrap().blocks.insert(h, b.clone());
        return Ok(Json(serde_json::json!(b)));
    }
    // Archive fallback.
    if let Some(arc) = &st.archive {
        let conn = arc.lock().unwrap();
        if let Some(ab) = archive_lookup(&conn, h) {
            return Ok(Json(serde_json::json!({"source": "archive", "block": ab})));
        }
    }
    Err(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct ArchiveBlocksQ {
    to: Option<u64>,
    n: Option<usize>,
}

async fn api_archive_status(State(st): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let conn = arc.lock().unwrap();
    let total: i64 = conn
        .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))
        .unwrap_or(0);
    let certs: i64 = conn
        .query_row("SELECT count(*) FROM blocks WHERE cert IS NOT NULL", [], |r| r.get(0))
        .unwrap_or(0);
    let typed_seq: Option<String> = conn
        .query_row("SELECT v FROM meta WHERE k='typed_seq'", [], |r| r.get(0))
        .ok();
    Ok(Json(serde_json::json!({
        "blocks": total,
        "with_certs": certs,
        "typed_seq": typed_seq,
    })))
}

/// Deep-history page of archive rows (seq/height/projection + cert info).
fn archive_blocks_page(conn: &rusqlite::Connection, to: u64, n: i64) -> Vec<serde_json::Value> {
    let Ok(mut stq) = conn.prepare(
        "SELECT seq,height,app_hash,state_root,proposer,cert FROM blocks WHERE height<=?1 ORDER BY height DESC LIMIT ?2",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stq.query_map([to as i64, n], |r| {
        let cert: Option<Vec<u8>> = r.get(5)?;
        let cert = cert.unwrap_or_default();
        let (hyb, signers, participants, mayo) = match parse_hybrid_envelope(&cert) {
            Ok(e) => (true, e.signers, e.participants, e.mayo_sig_count),
            Err(_) => (false, vec![], 0, 0),
        };
        Ok(serde_json::json!({
            "seq": r.get::<_, i64>(0)?,
            "height": r.get::<_, i64>(1)?,
            "app_hash": r.get::<_, Option<Vec<u8>>>(2)?.map(hex::encode),
            "state_root": r.get::<_, Option<Vec<u8>>>(3)?.map(hex::encode),
            "proposer": r.get::<_, Option<Vec<u8>>>(4)?.map(hex::encode),
            "cert_bytes": cert.len(),
            "hybrid_cert": hyb,
            "signers": signers,
            "participants": participants,
            "mayo_sigs": mayo,
        }))
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn archive_tip(conn: &rusqlite::Connection) -> u64 {
    conn.query_row("SELECT max(height) FROM blocks", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as u64
}

async fn api_archive_blocks(
    State(st): State<AppState>,
    Query(q): Query<ArchiveBlocksQ>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let n = q.n.unwrap_or(50).min(500) as i64;
    let rows = {
        let conn = arc.lock().unwrap();
        let to = q.to.unwrap_or_else(|| archive_tip(&conn));
        archive_blocks_page(&conn, to, n)
    };
    Ok(Json(serde_json::json!(rows)))
}

async fn api_address(
    State(st): State<AppState>,
    Path(addr): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let conn = arc.lock().unwrap();
    let mut stq = conn
        .prepare("SELECT denom, amount FROM balances WHERE address=?1")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let balances: Vec<serde_json::Value> = stq
        .query_map([&addr], |r| {
            Ok(serde_json::json!({"denom": r.get::<_, String>(0)?, "amount": r.get::<_, String>(1)?}))
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .flatten()
        .collect();
    let txs: Vec<serde_json::Value> = conn
        .prepare(
            "SELECT t.txhash, t.height, t.code FROM tx_addrs a \
             JOIN txs t ON t.txhash = a.txhash \
             WHERE a.address=?1 ORDER BY t.seq DESC LIMIT 50",
        )
        .and_then(|mut s| {
            let rows = s
                .query_map([&addr], |r| {
                    Ok(serde_json::json!({
                        "txhash": r.get::<_, String>(0)?,
                        "height": r.get::<_, Option<i64>>(1)?,
                        "code": r.get::<_, i64>(2)?,
                    }))
                })?
                .flatten()
                .collect();
            Ok(rows)
        })
        .unwrap_or_default();
    Ok(Json(serde_json::json!({"address": addr, "balances": balances, "txs": txs})))
}

async fn api_tx(
    State(st): State<AppState>,
    Path(hash): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let arc = st.archive.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let conn = arc.lock().unwrap();
    let row = conn
        .query_row(
            "SELECT txhash, seq, height, code, raw_log, gas_wanted, gas_used, response \
             FROM txs WHERE txhash=?1",
            [&hash],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<Vec<u8>>>(7)?,
                ))
            },
        )
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let events: Vec<serde_json::Value> = row
        .7
        .and_then(|b| TxResponse::decode(b.as_slice()).ok())
        .map(|r| {
            r.events
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "type": e.r#type,
                        "attributes": e.attributes.iter().map(|a| {
                            serde_json::json!({"key": a.key, "value": a.value})
                        }).collect::<Vec<_>>(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(serde_json::json!({
        "txhash": row.0,
        "seq": row.1,
        "height": row.2,
        "code": row.3,
        "raw_log": row.4,
        "gas_wanted": row.5,
        "gas_used": row.6,
        "events": events,
    })))
}

// ---------------------------------------------------------------------------
// Main: poll tip + backfill recent blocks, serve axum
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |f: &str| args.windows(2).find(|w| w[0] == f).map(|w| w[1].clone());
    let grpc = arg("--grpc").unwrap_or_else(|| "127.0.0.1:9090".into());
    let listen = arg("--listen").unwrap_or_else(|| "127.0.0.1:8081".into());
    let archive_path = arg("--archive");

    let archive = archive_path.map(|path| {
        // Read-only open — the archiver keeps writing (WAL).
        let conn = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap_or_else(|e| {
            eprintln!("[jc-explorer] archive open failed ({path}): {e} — deep history disabled");
            // Fall back to an in-memory empty DB so the Option stays usable.
            rusqlite::Connection::open_in_memory().expect("in-memory sqlite")
        });
        std::sync::Mutex::new(conn)
    });
    if archive.is_some() {
        eprintln!("[jc-explorer] archive attached — /history + deep /block fallback live");
    }

    let st = Arc::new(Shared2 {
        grpc: grpc.clone(),
        cache: RwLock::new(Cache {
            tip: 0,
            tip_time: None,
            blocks: HashMap::new(),
        }),
        archive,
    });

    // Poll tip every 2 s; backfill last ~150 blocks into the cache.
    {
        let grpc = grpc.clone();
        let cache = st.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(mut c) = lc_client(&grpc).await {
                    if let Ok(r) = c.latest_height(QueryLatestHeightRequest {}).await {
                        let tip = r.into_inner().height;
                        {
                            let mut g = cache.cache.write().unwrap();
                            g.tip = tip;
                            g.tip_time = Some(Instant::now());
                        }
                        // backfill newest unseen blocks, oldest-first batch of 20
                        let start = tip.saturating_sub(150);
                        for h in (start..=tip).rev().take(20) {
                            if cache.cache.read().unwrap().blocks.contains_key(&h) {
                                continue;
                            }
                            if let Some(b) = fetch_block(&grpc, h).await {
                                let mut g = cache.cache.write().unwrap();
                                g.blocks.insert(h, b);
                                // cap cache at 2000 entries — drop oldest
                                if g.blocks.len() > 2000 {
                                    if let Some(min) = g.blocks.keys().min().copied() {
                                        g.blocks.remove(&min);
                                    }
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    let app = Router::new()
        .route("/", get(home))
        .route("/block/:h", get(block_page))
        .route("/history", get(history_page))
        .route("/search", get(search))
        .route("/address/:a", get(address_page))
        .route("/tx/:hash", get(tx_page))
        .route("/api/status", get(api_status))
        .route("/api/blocks", get(api_blocks))
        .route("/api/block/:h", get(api_block))
        .route("/api/archive/status", get(api_archive_status))
        .route("/api/archive/blocks", get(api_archive_blocks))
        .route("/api/address/:a", get(api_address))
        .route("/api/tx/:hash", get(api_tx))
        .with_state(st);

    eprintln!("[jc-explorer] grpc={grpc} listening on http://{listen}");
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
