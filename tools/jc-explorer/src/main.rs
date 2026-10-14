//! jc-explorer — thin public explorer for JunoClaw.
//!
//! Server-rendered MVP: polls `layer.lightclient.v1.Query` for finalized
//! blocks, decodes the payload + hybrid certificate envelope, and serves
//! HTML + JSON from one binary. Put it behind the Caddy edge on OVH.
//!
//!   jc-explorer --grpc 127.0.0.1:9090 --listen 127.0.0.1:8081
//!
//! Pages:  /            status + recent blocks (auto-refresh)
//!         /block/{h}   block detail + hybrid cert panel
//! JSON:   /api/status  /api/blocks?n=  /api/block/{h}

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use axum::routing::get;
use axum::Router;
use layer_proto::layer::lightclient::v1::query_client::QueryClient as LcClient;
use layer_proto::layer::lightclient::v1::{
    QueryBlockRequest, QueryLatestHeightRequest,
};
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

type Shared = Arc<RwLock<Cache>>;

fn decode_block(height: u64, ts: u64, cert: &[u8], payload: &[u8]) -> BlockInfo {
    let (digest, tx_count, state_root, proposer) = match BlockPayload::from_bytes_ext(payload) {
        Some(p) => (
            hex::encode(Sha256::digest(payload)),
            p.txs.len(),
            hex::encode(p.state_root),
            hex::encode(&p.proposer[..p.proposer.len().min(8)]),
        ),
        None => ("—".into(), 0, "—".into(), "—".into()),
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

async fn home(State((_, cache)): State<(String, Shared)>) -> Html<String> {
    let (tip, blocks) = {
        let c = cache.read().unwrap();
        let mut v: Vec<BlockInfo> = c.blocks.values().cloned().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.height));
        (c.tip, v)
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
               <div class=pill><b>{}</b><span>blocks cached</span></div>\
               <div class=pill><b>BLS+MAYO2</b><span>certificate scheme</span></div>\
               <div class=pill><b>deterministic</b><span>CosmWasm execution</span></div>\
             </div>\
             <h2>Recent finalized blocks</h2>\
             <table><tr><th>Height</th><th>Age</th><th>Digest</th><th>Txs</th><th>Certificate</th><th>Cert bytes</th></tr>{rows}</table>\
             <div class=sub style='margin-top:22px'>auto-refresh 10s · thin community explorer — raw cert bytes at /api/block/{{h}} verify offline with tools/verify-cert</div>\
             <script>setTimeout(()=>location.reload(),10000)</script>",
            blocks.len()
        ),
    )
}

async fn block_page(
    State((grpc, cache)): State<(String, Shared)>,
    Path(h): Path<u64>,
) -> Result<Html<String>, StatusCode> {
    let b = {
        let hit = cache.read().unwrap().blocks.get(&h).cloned();
        match hit {
            Some(b) => b,
            None => {
                let b = fetch_block(&grpc, h).await.ok_or(StatusCode::NOT_FOUND)?;
                cache.write().unwrap().blocks.insert(h, b.clone());
                b
            }
        }
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

async fn api_status(State((_, cache)): State<(String, Shared)>) -> Json<serde_json::Value> {
    let c = cache.read().unwrap();
    Json(serde_json::json!({"tip": c.tip, "cached_blocks": c.blocks.len()}))
}

async fn api_blocks(
    State((_, cache)): State<(String, Shared)>,
    Query(q): Query<BlocksQ>,
) -> Json<serde_json::Value> {
    let c = cache.read().unwrap();
    let mut v: Vec<&BlockInfo> = c.blocks.values().collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.height));
    let n = q.n.unwrap_or(40).min(200);
    Json(serde_json::json!(v.iter().take(n).collect::<Vec<_>>()))
}

async fn api_block(
    State((grpc, cache)): State<(String, Shared)>,
    Path(h): Path<u64>,
) -> Result<Json<BlockInfo>, StatusCode> {
    if let Some(b) = cache.read().unwrap().blocks.get(&h) {
        return Ok(Json(b.clone()));
    }
    let b = fetch_block(&grpc, h).await.ok_or(StatusCode::NOT_FOUND)?;
    cache.write().unwrap().blocks.insert(h, b.clone());
    Ok(Json(b))
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

    let cache: Shared = Arc::new(RwLock::new(Cache {
        tip: 0,
        tip_time: None,
        blocks: HashMap::new(),
    }));

    // Poll tip every 2 s; backfill last ~150 blocks into the cache.
    {
        let grpc = grpc.clone();
        let cache = cache.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(mut c) = lc_client(&grpc).await {
                    if let Ok(r) = c.latest_height(QueryLatestHeightRequest {}).await {
                        let tip = r.into_inner().height;
                        {
                            let mut g = cache.write().unwrap();
                            g.tip = tip;
                            g.tip_time = Some(Instant::now());
                        }
                        // backfill newest unseen blocks, oldest-first batch of 20
                        let start = tip.saturating_sub(150);
                        for h in (start..=tip).rev().take(20) {
                            if cache.read().unwrap().blocks.contains_key(&h) {
                                continue;
                            }
                            if let Some(b) = fetch_block(&grpc, h).await {
                                let mut g = cache.write().unwrap();
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

    let state = (grpc.clone(), cache);
    let app = Router::new()
        .route("/", get(home))
        .route("/block/:h", get(block_page))
        .route("/api/status", get(api_status))
        .route("/api/blocks", get(api_blocks))
        .route("/api/block/:h", get(api_block))
        .with_state(state);

    eprintln!("[jc-explorer] grpc={grpc} listening on http://{listen}");
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
