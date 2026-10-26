//! jc-status — public status page for JunoClaw infrastructure.
//!
//! Periodically probes the chain's public surface and renders an honest
//! HTML status page + JSON API. Deliberately independent of the explorer:
//! it probes the public edge from inside, so a dead explorer shows as DOWN,
//! not as a missing page.
//!
//! Usage: jc-status --listen 127.0.0.1:8082 --grpc 127.0.0.1:9090
//!                  --public-grpc api.junoclaw.xyz:443 --explorer http://127.0.0.1:8081
//!                  [--seat-names anatoly,utsa,...]   (validator-index order)
//!
//! Alerts (stall on finalized height, per-seat certificate absence) go to
//! every configured sink: JC_ALERT_WEBHOOK (+ JC_ALERT_CHAT_ID for the
//! Telegram Bot API) and/or JC_ALERT_CMD (alert text on stdin).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, Json};
use axum::routing::get;
use axum::Router;
use layer_proto::cosmwasm::wasm::v1::{
    query_client::QueryClient as WasmQueryClient, QuerySmartContractStateRequest,
};
use layer_proto::layer::sync::v1::query_client::QueryClient as SyncQueryClient;
use layer_proto::layer::sync::v1::QueryLatestSequenceRequest;
use std::collections::HashMap;
use serde::Serialize;
use tonic::transport::{Channel, ClientTlsConfig};

const PROBE_INTERVAL: Duration = Duration::from_secs(30);
const HISTORY_LEN: usize = 2880; // 24 h of 30 s probes
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
/// Tip older than this means the chain is not finalizing — alert.
const STALL_ALERT_SECS: u64 = 120;
/// A seat absent from every observed certificate this long is alerted.
/// Slow seats land in ~20% of certs, so they still appear within minutes.
const SEAT_ALERT_SECS: u64 = 600;

/// Per-seat certificate liveness, keyed by validator index.
#[derive(Clone, Serialize)]
struct Seat {
    index: usize,
    name: String,
    last_height: u64,
    last_seen: u64,
    #[serde(skip)]
    alerted: bool,
}

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
    /// Latest finalized block height, as reported by the explorer probe.
    height: Mutex<u64>,
    /// When the finalized height last advanced.
    height_seen_at: Mutex<u64>,
    /// Webhook URL (JC_ALERT_WEBHOOK) that receives stall/recovery alerts.
    alert_webhook: Option<String>,
    /// Telegram chat id (JC_ALERT_CHAT_ID), sent as `chat_id` with the webhook body.
    alert_chat_id: Option<String>,
    /// Command (JC_ALERT_CMD) run via `sh -c` with the alert text on stdin.
    alert_cmd: Option<String>,
    /// Whether a stall alert has already fired for the current stall episode.
    stall_alerted: Mutex<bool>,
    /// Seat display names in validator-index order (--seat-names).
    seat_names: Vec<String>,
    /// Per-seat liveness from observed certificates.
    seats: Mutex<Vec<Seat>>,
    /// Local explorer address, for the certificate signer feed.
    explorer: String,
    /// Local consensus gRPC for the wasm smart-query passthrough.
    grpc: String,
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

/// Deliver an alert to every configured sink. With no sink configured the
/// alert is still logged, so the journal keeps the record.
async fn dispatch(sh: &Shared, text: &str) {
    eprintln!("[jc-status] ALERT {text}");
    if let Some(webhook) = &sh.alert_webhook {
        send_alert(webhook, sh.alert_chat_id.as_deref(), text).await;
    }
    if let Some(cmd) = &sh.alert_cmd {
        run_alert_cmd(cmd, text).await;
    }
}

/// Run `sh -c <cmd>` with the alert text on stdin (e.g. a Buzz poster).
async fn run_alert_cmd(cmd: &str, text: &str) {
    use tokio::io::AsyncWriteExt;
    let child = tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return eprintln!("[jc-status] alert cmd spawn failed: {e}"),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes()).await;
    }
    match tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await {
        Ok(Ok(o)) if o.status.success() => {}
        Ok(Ok(o)) => eprintln!(
            "[jc-status] alert cmd failed: status={} stderr={}",
            o.status,
            String::from_utf8_lossy(&o.stderr)
        ),
        Ok(Err(e)) => eprintln!("[jc-status] alert cmd error: {e}"),
        Err(_) => eprintln!("[jc-status] alert cmd timed out"),
    }
}

/// POST a JSON alert to the configured webhook via curl (handles HTTPS
/// endpoints like Telegram/Discord without adding a client dependency).
/// With a chat id the body matches the Telegram Bot API `sendMessage`.
async fn send_alert(webhook: &str, chat_id: Option<&str>, text: &str) {
    let body = match chat_id {
        Some(id) => serde_json::json!({"chat_id": id, "text": text}),
        None => serde_json::json!({"text": text}),
    }
    .to_string();
    let out = tokio::process::Command::new("curl")
        .args([
            "-sS",
            "-m",
            "10",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            webhook,
        ])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!(
            "[jc-status] alert POST failed: status={} stderr={}",
            o.status,
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => eprintln!("[jc-status] alert curl spawn failed: {e}"),
    }
}

/// Fire/recover stall alerts. The chain counts as stalled only when neither
/// the finalized height nor the local state sequence has advanced, so a
/// frozen explorer or a wedged local node alone does not page as a stall.
async fn check_stall(sh: &Shared) {
    let t = now();
    let height_age = t.saturating_sub(*sh.height_seen_at.lock().unwrap());
    let seq_age = t.saturating_sub(*sh.tip_seen_at.lock().unwrap());
    let age = height_age.min(seq_age);
    let height = *sh.height.lock().unwrap();
    let msg = {
        let mut alerted = sh.stall_alerted.lock().unwrap();
        if age > STALL_ALERT_SECS && !*alerted {
            *alerted = true;
            Some(format!(
                "JunoClaw STALL: finalized height {height} has not advanced for {height_age}s (state sequence idle {seq_age}s)"
            ))
        } else if age <= STALL_ALERT_SECS && *alerted {
            *alerted = false;
            // Seats could not sign while the chain was stalled; restart their clocks.
            for s in sh.seats.lock().unwrap().iter_mut() {
                s.last_seen = t;
            }
            Some(format!("JunoClaw RECOVERED: finalizing again at height {height}"))
        } else {
            None
        }
    };
    if let Some(text) = msg {
        dispatch(sh, &text).await;
    }
}

fn seat_label(s: &Seat) -> String {
    if s.name.is_empty() {
        format!("idx{}", s.index)
    } else {
        format!("idx{} ({})", s.index, s.name)
    }
}

/// Record which seats signed the recently finalized certificates.
fn update_seats(sh: &Shared, blocks: &[serde_json::Value]) {
    let t = now();
    let mut seats = sh.seats.lock().unwrap();
    for b in blocks {
        let Some(signers) = b.get("signers").and_then(|v| v.as_array()) else {
            continue;
        };
        let height = b.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
        let participants = b.get("participants").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        while seats.len() < participants {
            let index = seats.len();
            seats.push(Seat {
                index,
                name: sh.seat_names.get(index).cloned().unwrap_or_default(),
                last_height: 0,
                last_seen: t,
                alerted: false,
            });
        }
        for idx in signers.iter().filter_map(|v| v.as_u64()) {
            if let Some(s) = seats.get_mut(idx as usize) {
                if height > s.last_height {
                    s.last_height = height;
                    s.last_seen = t;
                }
            }
        }
    }
}

/// Fire/recover per-seat absence alerts. Suppressed during a chain stall,
/// when no seat can sign.
async fn check_seats(sh: &Shared) {
    if *sh.stall_alerted.lock().unwrap() {
        return;
    }
    let t = now();
    let msgs: Vec<String> = {
        let mut seats = sh.seats.lock().unwrap();
        seats
            .iter_mut()
            .filter_map(|s| {
                let absent = t.saturating_sub(s.last_seen);
                if absent > SEAT_ALERT_SECS && !s.alerted {
                    s.alerted = true;
                    Some(format!(
                        "JunoClaw SEAT ABSENT: {} has signed no certificate for {}m (last at height {})",
                        seat_label(s),
                        absent / 60,
                        s.last_height
                    ))
                } else if absent <= SEAT_ALERT_SECS && s.alerted {
                    s.alerted = false;
                    Some(format!(
                        "JunoClaw SEAT BACK: {} signing again at height {}",
                        seat_label(s),
                        s.last_height
                    ))
                } else {
                    None
                }
            })
            .collect()
    };
    for m in msgs {
        dispatch(sh, &m).await;
    }
}

/// Pull the explorer's recent finalized blocks and update seat liveness.
async fn poll_signers(sh: &Shared) {
    let Ok(resp) = http_probe(&sh.explorer, "/api/blocks").await else {
        return;
    };
    let Some(body) = resp.split("\r\n\r\n").nth(1) else {
        return;
    };
    if let Ok(serde_json::Value::Array(blocks)) = serde_json::from_str(body.trim()) {
        update_seats(sh, &blocks);
    }
}

/// Availability over the retained history window (at most 24 h):
/// (window hours, % of rounds the consensus probe was up, % of rounds every probe was up).
fn availability(hist: &VecDeque<Snapshot>) -> (f64, f64, f64) {
    let total = hist.len();
    if total == 0 {
        return (0.0, 0.0, 0.0);
    }
    let chain_up = hist
        .iter()
        .filter(|s| s.probes.iter().any(|p| p.name.starts_with("Consensus") && p.ok))
        .count();
    let all_up = hist.iter().filter(|s| s.probes.iter().all(|p| p.ok)).count();
    let pct = |n: usize| n as f64 * 100.0 / total as f64;
    (
        total as f64 * PROBE_INTERVAL.as_secs_f64() / 3600.0,
        pct(chain_up),
        pct(all_up),
    )
}

fn render(sh: &Shared) -> String {
    let hist = sh.history.lock().unwrap();
    let (window_h, chain_pct, all_pct) = availability(&hist);
    let height = *sh.height.lock().unwrap();
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

    let t = now();
    let seat_rows: String = sh
        .seats
        .lock()
        .unwrap()
        .iter()
        .map(|s| {
            let absent = t.saturating_sub(s.last_seen);
            let ok = absent <= SEAT_ALERT_SECS;
            format!(
                "<tr><td>{}</td><td class='{}'>{}</td><td>{}</td><td>{} s ago</td></tr>",
                seat_label(s).replace('<', "&lt;"),
                if ok { "ok" } else { "bad" },
                if ok { "SIGNING" } else { "ABSENT" },
                s.last_height,
                absent
            )
        })
        .collect();

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
<div class='meta'>Finalized height {} &middot; state sequence {} &middot; chain last advanced {} s ago &middot; monitored since {}</div>
<div class='meta'>Availability, last {:.1} h: chain {:.1}% &middot; all components {:.1}%</div>
<div class='bars'>{}</div>
<table><tr><th>Component</th><th>Status</th><th>Detail</th><th>Latency</th></tr>{}</table>
<h2 style='font-size:16px;margin-top:28px'>Validator seats</h2>
<table><tr><th>Seat</th><th>Status</th><th>Last signed height</th><th>Last seen</th></tr>{}</table>
<p class='meta'>Each certificate carries a quorum of signatures, so a live seat is not in every
certificate; a seat is ABSENT after {} min without appearing in any observed certificate.</p>
<p class='meta'>Probes run every 30 s from the OVH edge host. Bars cover the last {} h.
Sources: <a href='https://explorer.junoclaw.xyz'>explorer</a> &middot;
<a href='https://api.junoclaw.xyz'>api</a> &middot;
<a href='/api/status'>json</a></p>
</body></html>",
        banner.1,
        banner.1,
        banner.0,
        height,
        tip,
        tip_age,
        sh.started,
        window_h,
        chain_pct,
        all_pct,
        bars,
        rows,
        seat_rows,
        SEAT_ALERT_SECS / 60,
        HISTORY_LEN / 120
    )
}

async fn page(State(sh): State<Arc<Shared>>) -> Html<String> {
    Html(render(&sh))
}

async fn api(State(sh): State<Arc<Shared>>) -> Json<serde_json::Value> {
    let hist = sh.history.lock().unwrap();
    let (window_h, chain_pct, all_pct) = availability(&hist);
    let seq = *sh.last_tip.lock().unwrap();
    Json(serde_json::json!({
        "latest": hist.back(),
        "height": *sh.height.lock().unwrap(),
        "sequence": seq,
        "tip_age_s": now().saturating_sub(*sh.tip_seen_at.lock().unwrap()),
        "window_hours": window_h,
        "chain_availability_pct": chain_pct,
        "all_components_pct": all_pct,
        "probes": hist.len(),
        "height_age_s": now().saturating_sub(*sh.height_seen_at.lock().unwrap()),
        "seats": *sh.seats.lock().unwrap(),
        // Deprecated aliases kept for older clients: `tip` is the state
        // sequence (not a block height); the uptime figure covers the rolling
        // history window (<= 24 h), not the process lifetime.
        "tip": seq,
        "uptime_pct_since_start": all_pct,
    }))
}

/// Base64 decode (standard + url-safe alphabets, padding optional).
fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut v = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut nbits = 0u32;
    for &c in s.as_bytes() {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | d;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            v.push((acc >> nbits) as u8);
        }
    }
    Some(v)
}

fn b64_encode(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | (*c.get(2).unwrap_or(&0) as u32);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

/// GET /api/wasm/smart/{contract}?query={base64-or-json}
///
/// Read-only CosmWasm SmartContractState passthrough so browser frontends
/// (dao.junoclaw.xyz) can read contract state without a gRPC client. The
/// `query` param accepts standard/url-safe base64 or a raw JSON object
/// (percent-encoded). Responses mirror the LCD shape: {"data": {...}}.
async fn wasm_smart(
    State(sh): State<Arc<Shared>>,
    Path(contract): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let bad = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
    };
    if !contract.starts_with("juno1") || contract.len() < 40 || contract.len() > 70 {
        return Err(bad("contract must be a juno1 bech32 address"));
    }
    let raw = params
        .get("query")
        .ok_or_else(|| bad("missing ?query="))?;
    let query_data = if raw.trim_start().starts_with('{') {
        raw.as_bytes().to_vec()
    } else {
        b64_decode(raw).ok_or_else(|| bad("query is not base64 or json"))?
    };
    if query_data.len() > 4096 {
        return Err(bad("query too large"));
    }

    let ep = Channel::from_shared(format!("http://{}", sh.grpc))
        .map_err(|e| bad(&e.to_string()))?;
    let conn = tokio::time::timeout(PROBE_TIMEOUT, ep.connect())
        .await
        .map_err(|_| bad("grpc connect timeout"))?
        .map_err(|e| bad(&e.to_string()))?;
    let mut client = WasmQueryClient::new(conn);
    let resp = tokio::time::timeout(
        PROBE_TIMEOUT,
        client.smart_contract_state(QuerySmartContractStateRequest {
            address: contract,
            query_data,
        }),
    )
    .await
    .map_err(|_| bad("query timeout"))?
    .map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": e.message()})),
        )
    })?
    .into_inner();

    // Contract responses are utf8 JSON; fall back to base64 for binary.
    let data = match serde_json::from_slice::<serde_json::Value>(&resp.data) {
        Ok(v) => v,
        Err(_) => serde_json::Value::String(b64_encode(&resp.data)),
    };
    Ok(Json(serde_json::json!({"data": data})))
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
    let seat_names: Vec<String> = arg("--seat-names", "")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let env = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty());

    let sh = Arc::new(Shared {
        history: Mutex::new(VecDeque::with_capacity(HISTORY_LEN)),
        started: now(),
        last_tip: Mutex::new(0),
        tip_seen_at: Mutex::new(now()),
        height: Mutex::new(0),
        height_seen_at: Mutex::new(now()),
        alert_webhook: env("JC_ALERT_WEBHOOK"),
        alert_chat_id: env("JC_ALERT_CHAT_ID"),
        alert_cmd: env("JC_ALERT_CMD"),
        stall_alerted: Mutex::new(false),
        seat_names,
        seats: Mutex::new(Vec::new()),
        explorer: explorer.clone(),
        grpc: grpc.clone(),
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
                    let mut tip_seen = sh.tip_seen_at.lock().unwrap();
                    if seq > *sh.last_tip.lock().unwrap() {
                        *tip_seen = now();
                    }
                    drop(tip_seen);
                    *sh.last_tip.lock().unwrap() = seq;
                }
                if let Some(h) = probes
                    .iter()
                    .find(|p| p.name.starts_with("Explorer") && p.ok)
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(&p.detail).ok())
                    .and_then(|v| v.get("tip").and_then(|t| t.as_u64()))
                {
                    let mut height = sh.height.lock().unwrap();
                    if h > *height {
                        *sh.height_seen_at.lock().unwrap() = now();
                    }
                    *height = h;
                }
                poll_signers(&sh).await;
                check_stall(&sh).await;
                check_seats(&sh).await;
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
        .route("/api/wasm/smart/:contract", get(wasm_smart))
        .with_state(sh);
    eprintln!("[jc-status] listening on {listen}");
    let l = tokio::net::TcpListener::bind(&listen).await.unwrap();
    axum::serve(l, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared(names: &[&str]) -> Shared {
        Shared {
            history: Mutex::new(VecDeque::new()),
            started: now(),
            last_tip: Mutex::new(0),
            tip_seen_at: Mutex::new(now()),
            height: Mutex::new(0),
            height_seen_at: Mutex::new(now()),
            alert_webhook: None,
            alert_chat_id: None,
            alert_cmd: None,
            stall_alerted: Mutex::new(false),
            seat_names: names.iter().map(|s| s.to_string()).collect(),
            seats: Mutex::new(Vec::new()),
            explorer: String::new(),
            grpc: String::new(),
        }
    }

    fn block(height: u64, signers: &[u64]) -> serde_json::Value {
        serde_json::json!({"height": height, "participants": 3, "signers": signers})
    }

    #[test]
    fn seats_track_last_signed_height() {
        let sh = shared(&["a", "b", "c"]);
        update_seats(&sh, &[block(10, &[0, 1]), block(9, &[0, 2])]);
        let seats = sh.seats.lock().unwrap();
        assert_eq!(seats.len(), 3);
        assert_eq!(seats[0].last_height, 10);
        assert_eq!(seats[1].last_height, 10);
        assert_eq!(seats[2].last_height, 9);
        assert_eq!(seat_label(&seats[2]), "idx2 (c)");
    }

    #[test]
    fn uncertified_blocks_are_ignored() {
        let sh = shared(&[]);
        update_seats(&sh, &[serde_json::json!({"height": 5, "participants": 0, "signers": []})]);
        assert!(sh.seats.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn absent_seat_alerts_once_then_recovers() {
        let sh = shared(&["a", "b", "c"]);
        update_seats(&sh, &[block(10, &[0, 1, 2])]);
        sh.seats.lock().unwrap()[2].last_seen = now() - SEAT_ALERT_SECS - 1;

        check_seats(&sh).await;
        assert!(sh.seats.lock().unwrap()[2].alerted);
        assert!(!sh.seats.lock().unwrap()[0].alerted);

        // Still absent: no second alert, state unchanged.
        check_seats(&sh).await;
        assert!(sh.seats.lock().unwrap()[2].alerted);

        update_seats(&sh, &[block(11, &[0, 2])]);
        check_seats(&sh).await;
        assert!(!sh.seats.lock().unwrap()[2].alerted);
        assert_eq!(sh.seats.lock().unwrap()[2].last_height, 11);
    }

    #[tokio::test]
    async fn stall_suppresses_seat_alerts_and_recovery_resets_clocks() {
        let sh = shared(&["a"]);
        update_seats(&sh, &[serde_json::json!({"height": 1, "participants": 1, "signers": [0]})]);
        let old = now() - SEAT_ALERT_SECS - 1;
        sh.seats.lock().unwrap()[0].last_seen = old;
        *sh.height_seen_at.lock().unwrap() = now() - STALL_ALERT_SECS - 1;
        *sh.tip_seen_at.lock().unwrap() = now() - STALL_ALERT_SECS - 1;

        check_stall(&sh).await;
        assert!(*sh.stall_alerted.lock().unwrap());
        check_seats(&sh).await;
        assert!(!sh.seats.lock().unwrap()[0].alerted);

        // Finalized height advances again: the stall clears and seat clocks restart.
        *sh.height_seen_at.lock().unwrap() = now();
        check_stall(&sh).await;
        assert!(!*sh.stall_alerted.lock().unwrap());
        assert!(sh.seats.lock().unwrap()[0].last_seen > old);
    }
}
