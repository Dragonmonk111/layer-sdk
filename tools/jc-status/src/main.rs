//! jc-status — public status page for JunoClaw infrastructure.
//!
//! Periodically probes the chain's public surface and renders an honest
//! HTML status page + JSON API. Deliberately independent of the explorer:
//! it probes the public edge from inside, so a dead explorer shows as DOWN,
//! not as a missing page.
//!
//! Usage: jc-status --listen 127.0.0.1:8082 --grpc 127.0.0.1:9090
//!                  --public-grpc api.junoclaw.xyz:443 --explorer http://127.0.0.1:8081

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::response::{Html, Json};
use axum::routing::get;
use axum::Router;
use layer_proto::layer::sync::v1::query_client::QueryClient as SyncQueryClient;
use layer_proto::layer::sync::v1::QueryLatestSequenceRequest;
use serde::Serialize;
use tonic::transport::{Channel, ClientTlsConfig};

const PROBE_INTERVAL: Duration = Duration::from_secs(30);
const HISTORY_LEN: usize = 2880; // 24 h of 30 s probes
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Serialize)]
struct Probe {
    name: &'static str,
    ok: bool,
    detail: String,
    ms: u128,
}

#[derive(Clone, Serialize)]
struct Snapshot {
    ts: u64,
    probes: Vec<Probe>,
}

struct Shared {
    history: Mutex<VecDeque<Snapshot>>,
    started: u64,
    last_tip: Mutex<u64>,
    tip_seen_at: Mutex<u64>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

async fn latest_seq(grpc: &str, tls: bool) -> Result<u64, String> {
    let uri = if grpc.contains("://") {
        grpc.to_string()
    } else {
        format!("{}://{grpc}", if tls { "https" } else { "http" })
    };
    let mut ep = Channel::from_shared(uri).map_err(|e| e.to_string())?;
    if tls {
        ep = ep
            .tls_config(ClientTlsConfig::new().with_webpki_roots())
            .map_err(|e| e.to_string())?;
    }
    let conn = tokio::time::timeout(PROBE_TIMEOUT, ep.connect())
        .await
        .map_err(|_| "connect timeout".to_string())?
        .map_err(|e| e.to_string())?;
    let mut client = SyncQueryClient::new(conn);
    let r = tokio::time::timeout(
        PROBE_TIMEOUT,
        client.latest_sequence(QueryLatestSequenceRequest {}),
    )
    .await
    .map_err(|_| "query timeout".to_string())?
    .map_err(|e| e.to_string())?;
    Ok(r.into_inner().sequence)
}

/// Minimal HTTP/1.1 GET for the local explorer (plain http, same host).
async fn http_probe(addr: &str, path: &str) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    let mut s = tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(addr))
        .await
        .map_err(|_| "connect timeout".to_string())?
        .map_err(|e| e.to_string())?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    tokio::time::timeout(PROBE_TIMEOUT, s.read_to_end(&mut buf))
        .await
        .map_err(|_| "read timeout".to_string())?
        .map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

async fn probe_all(grpc: String, public: String, explorer: String) -> Vec<Probe> {
    let mut out = Vec::new();

    let t = Instant::now();
    let p = match latest_seq(&grpc, false).await {
        Ok(seq) => Probe {
            name: "Consensus node (gRPC :9090)",
            ok: true,
            detail: format!("sequence {seq}"),
            ms: t.elapsed().as_millis(),
        },
        Err(e) => Probe {
            name: "Consensus node (gRPC :9090)",
            ok: false,
            detail: e,
            ms: t.elapsed().as_millis(),
        },
    };
    out.push(p);

    let t = Instant::now();
    let p = match latest_seq(&public, true).await {
        Ok(seq) => Probe {
            name: "Public API (api.junoclaw.xyz)",
            ok: true,
            detail: format!("TLS ok, sequence {seq}"),
            ms: t.elapsed().as_millis(),
        },
        Err(e) => Probe {
            name: "Public API (api.junoclaw.xyz)",
            ok: false,
            detail: e,
            ms: t.elapsed().as_millis(),
        },
    };
    out.push(p);

    let t = Instant::now();
    let p = match http_probe(&explorer, "/api/status").await {
        Ok(body) => {
            let ok = body.contains("HTTP/1.1 200") || body.contains("HTTP/1.0 200");
            let detail = body
                .split("\r\n\r\n")
                .nth(1)
                .map(|b| b.trim().chars().take(160).collect())
                .unwrap_or_else(|| "no body".into());
            Probe {
                name: "Explorer (explorer.junoclaw.xyz)",
                ok,
                detail,
                ms: t.elapsed().as_millis(),
            }
        }
        Err(e) => Probe {
            name: "Explorer (explorer.junoclaw.xyz)",
            ok: false,
            detail: e,
            ms: t.elapsed().as_millis(),
        },
    };
    out.push(p);
    out
}

fn render(sh: &Shared) -> String {
    let hist = sh.history.lock().unwrap();
    let latest = hist.back();
    let all_ok = latest.map(|s| s.probes.iter().all(|p| p.ok)).unwrap_or(false);
    let tip = *sh.last_tip.lock().unwrap();
    let tip_age = now().saturating_sub(*sh.tip_seen_at.lock().unwrap());
    let banner = if latest.is_none() {
        ("PROBING", "#f59e0b")
    } else if all_ok && tip_age < 60 {
        ("ALL SYSTEMS OPERATIONAL", "#22c55e")
    } else {
        ("DEGRADED", "#ef4444")
    };

    // 24h bar strip: one cell per probe round, newest at right
    let bars: String = hist
        .iter()
        .map(|s| {
            let ok = s.probes.iter().all(|p| p.ok);
            format!(
                "<div class='bar {}' title='{}'></div>",
                if ok { "up" } else { "down" },
                s.ts
            )
        })
        .collect();

    let rows: String = latest
        .map(|s| {
            s.probes
                .iter()
                .map(|p| {
                    format!(
                        "<tr><td>{}</td><td class='{}'>{}</td><td>{}</td><td>{} ms</td></tr>",
                        p.name,
                        if p.ok { "ok" } else { "bad" },
                        if p.ok { "UP" } else { "DOWN" },
                        p.detail
                            .replace('&', "&amp;")
                            .replace('<', "&lt;"),
                        p.ms
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_default();

    format!(
        "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'>
<meta http-equiv='refresh' content='30'><title>JunoClaw Status</title><style>
body{{background:#0b0e14;color:#e6e6e6;font:15px/1.5 ui-monospace,Menlo,monospace;max-width:820px;margin:40px auto;padding:0 16px}}
h1{{font-size:20px}} .banner{{padding:14px 18px;border-radius:8px;font-weight:700;margin:16px 0}}
table{{width:100%;border-collapse:collapse}} td,th{{padding:8px;border-bottom:1px solid #222;text-align:left}}
.ok{{color:#22c55e}} .bad{{color:#ef4444}}
.bars{{display:flex;gap:1px;margin:10px 0 24px}} .bar{{width:4px;height:28px;background:#333}}
.bar.up{{background:#22c55e}} .bar.down{{background:#ef4444}}
.meta{{color:#888;font-size:13px}} a{{color:#8ab4f8}}
</style></head><body>
<h1>JunoClaw Network Status</h1>
<div class='banner' style='background:{}22;color:{}'>{}</div>
<div class='meta'>Chain tip: sequence {} &middot; last probe {} s ago &middot; monitored since {}</div>
<div class='bars'>{}</div>
<table><tr><th>Component</th><th>Status</th><th>Detail</th><th>Latency</th></tr>{}</table>
<p class='meta'>Probes run every 30 s from the OVH edge host. Bars cover the last {} h.
Sources: <a href='https://explorer.junoclaw.xyz'>explorer</a> &middot;
<a href='https://api.junoclaw.xyz'>api</a> &middot;
<a href='/api/status'>json</a></p>
</body></html>",
        banner.1,
        banner.1,
        banner.0,
        tip,
        tip_age,
        sh.started,
        bars,
        rows,
        HISTORY_LEN / 120
    )
}

async fn page(State(sh): State<Arc<Shared>>) -> Html<String> {
    Html(render(&sh))
}

async fn api(State(sh): State<Arc<Shared>>) -> Json<serde_json::Value> {
    let hist = sh.history.lock().unwrap();
    let total = hist.len();
    let up = hist
        .iter()
        .filter(|s| s.probes.iter().all(|p| p.ok))
        .count();
    Json(serde_json::json!({
        "latest": hist.back(),
        "uptime_pct_since_start": if total>0 { up as f64 * 100.0 / total as f64 } else { 0.0 },
        "probes": total,
        "tip": *sh.last_tip.lock().unwrap(),
    }))
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |f: &str, d: &str| {
        args.windows(2)
            .find(|w| w[0] == f)
            .map(|w| w[1].clone())
            .unwrap_or_else(|| d.to_string())
    };
    let listen = arg("--listen", "127.0.0.1:8082");
    let grpc = arg("--grpc", "127.0.0.1:9090");
    let public = arg("--public-grpc", "api.junoclaw.xyz:443");
    let explorer = arg("--explorer", "127.0.0.1:8081");

    let sh = Arc::new(Shared {
        history: Mutex::new(VecDeque::with_capacity(HISTORY_LEN)),
        started: now(),
        last_tip: Mutex::new(0),
        tip_seen_at: Mutex::new(now()),
    });

    {
        let sh = sh.clone();
        tokio::spawn(async move {
            loop {
                let probes = probe_all(grpc.clone(), public.clone(), explorer.clone()).await;
                if let Some(seq) = probes
                    .iter()
                    .find(|p| p.name.starts_with("Consensus"))
                    .and_then(|p| {
                        p.detail
                            .strip_prefix("sequence ")
                            .and_then(|s| s.parse::<u64>().ok())
                    })
                {
                    *sh.last_tip.lock().unwrap() = seq;
                    *sh.tip_seen_at.lock().unwrap() = now();
                }
                {
                    let mut h = sh.history.lock().unwrap();
                    if h.len() >= HISTORY_LEN {
                        h.pop_front();
                    }
                    h.push_back(Snapshot { ts: now(), probes });
                }
                tokio::time::sleep(PROBE_INTERVAL).await;
            }
        });
    }

    let app = Router::new()
        .route("/", get(page))
        .route("/api/status", get(api))
        .with_state(sh);
    eprintln!("[jc-status] listening on {listen}");
    let l = tokio::net::TcpListener::bind(&listen).await.unwrap();
    axum::serve(l, app).await.unwrap();
}
