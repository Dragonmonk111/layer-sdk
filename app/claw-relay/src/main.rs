//! claw-relay — JunoClaw sovereign coordination relay.
//!
//! Nostr wire protocol over WebSocket, wire-compatible with buzz-cli:
//!   client -> ["EVENT",{event}] | ["REQ","subid",{filter}...] | ["CLOSE","subid"]
//!   relay  -> ["OK",id,accepted,"msg"] | ["EVENT","subid",{event}]
//!             | ["EOSE","subid"] | ["NOTICE","msg"]
//!
//! Admission: writes require the event's pubkey in the active
//! allowlist AND an `h` tag naming a configured channel. Reads are
//! open — transcripts are meant to be verifiable by anyone.
//!
//! The allowlist is derived on-chain when `--cw4-group` is set: a
//! background task polls the cw4 contract's `list_members` smart query
//! and maps members to npubs via `--member-bindings`. The gate fails
//! CLOSED — an empty effective set denies all writes — and keeps the
//! last-good set on refresh failures. `--allowlist` entries are
//! additive break-glass npubs (e.g. the relay operator), merged into
//! every refresh.
//!
//! Usage:
//!   claw-relay --listen 127.0.0.1:7070 --db /var/lib/claw/relay.redb \
//!     --channels <uuid1,uuid2,...> --allowlist /etc/claw/allowlist.txt \
//!     --cw4-group juno1... --chain-api http://127.0.0.1:8082 \
//!     --member-bindings /etc/claw/member-bindings.json \
//!     --cw4-refresh-secs 60 \
//!     --peer ws://relay-b.example:7070 [--peer ws://relay-c.example:7070]
//!
//! Federation (v1, pull-only): each `--peer <ws-url>` spawns a task
//! that opens a WebSocket to the peer and REQs every configured
//! channel. Remote events pass the same signature + admission checks
//! as client writes before hitting the store — dedupe by id at each
//! hop is the loop-safety mechanism; relays never push.

mod admission;
mod gossip;
mod store;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use claw_core::{Event, Filter};
use futures_channel::mpsc;
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use store::Store;
use tokio::sync::{broadcast, RwLock};

/// Kinds accepted for channel writes — mirrors the set buzz-cli reads by
/// default: 9 chat, 40002/40008 channel msgs, 45001/45003 mod events.
const ALLOWED_KINDS: &[u64] = &[9, 40002, 40008, 45001, 45003];

struct App {
    store: Store,
    channels: HashSet<String>,
    /// Effective write-admission set (last-good cw4 npubs ∪ break-glass
    /// file entries). Arc'd so the cw4 poller task can share it.
    /// Empty = deny ALL writes (fail-closed).
    allowlist: Arc<RwLock<HashSet<String>>>,
    tx: broadcast::Sender<Arc<Event>>,
}

type Shared = Arc<App>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "claw_relay=info".into()),
    ).init();

    let args: Vec<String> = std::env::args().collect();
    let opt = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let listen: SocketAddr = opt("--listen")
        .unwrap_or_else(|| "127.0.0.1:7070".into())
        .parse()?;
    let db = PathBuf::from(opt("--db").unwrap_or_else(|| "relay.redb".into()));
    let channels: HashSet<String> = opt("--channels")
        .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
        .unwrap_or_default();
    let allowlist_path = opt("--allowlist").map(PathBuf::from);
    // repeatable: --peer ws://host:port --peer ws://other:port
    let peers: Vec<String> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--peer")
        .filter_map(|(i, _)| args.get(i + 1).cloned())
        .collect();
    // static break-glass npubs — also the initial effective set, so a
    // fresh relay denies everyone except operator entries until the
    // first successful cw4 refresh.
    let breakglass = load_allowlist(allowlist_path.as_deref());

    // on-chain admission flags
    let cw4_group = opt("--cw4-group");
    let chain_api = opt("--chain-api").unwrap_or_else(|| "http://127.0.0.1:8082".into());
    let bindings_path = opt("--member-bindings").map(PathBuf::from);
    let cw4_refresh_secs: u64 = opt("--cw4-refresh-secs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60)
        .max(5);

    let app = Arc::new(App {
        store: Store::open(&db)?,
        channels,
        allowlist: Arc::new(RwLock::new(breakglass.clone())),
        tx: broadcast::channel(4096).0,
    });

    if let Some(group_addr) = cw4_group {
        if bindings_path.is_none() {
            tracing::warn!(
                "--cw4-group set without --member-bindings: no member can map to an npub; writes stay closed"
            );
        }
        let cfg = admission::Cw4Config {
            api_base: chain_api.clone(),
            group_addr: group_addr.clone(),
            bindings_path,
            refresh: Duration::from_secs(cw4_refresh_secs),
            breakglass,
        };
        tokio::spawn(admission::poll_loop(cfg, app.allowlist.clone()));
        tracing::info!(%group_addr, %chain_api, cw4_refresh_secs, "cw4 admission poller started");
    }

    // federation: one pull-only gossip task per --peer. Peers REQ on
    // the configured channels, so --channels must be non-empty.
    if !peers.is_empty() {
        if app.channels.is_empty() {
            tracing::warn!(
                peers = peers.len(),
                "--peer set without --channels: gossip skipped (no channels to subscribe)"
            );
        } else {
            for peer in peers {
                tracing::info!(%peer, "gossip peer configured");
                tokio::spawn(gossip::run_peer(app.clone(), peer));
            }
        }
    }

    let router = Router::new()
        .route("/", get(ws_handler))
        .route("/events", axum::routing::post(http_events))
        .route("/query", axum::routing::post(http_query))
        .route("/count", axum::routing::post(http_count))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(app);

    tracing::info!(%listen, ?db, "claw-relay listening");
    axum::serve(tokio::net::TcpListener::bind(listen).await?, router).await?;
    Ok(())
}

fn load_allowlist(path: Option<&std::path::Path>) -> HashSet<String> {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .map(|l| l.trim().to_lowercase())
                .filter(|l| l.len() == 64 && l.chars().all(|c| c.is_ascii_hexdigit()))
                .collect()
        })
        .unwrap_or_default()
}

async fn ws_handler(
    ws: Option<WebSocketUpgrade>,
    headers: axum::http::HeaderMap,
    State(app): State<Shared>,
) -> axum::response::Response {
    // NIP-11: relay information document on `Accept: application/nostr+json`
    if headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("application/nostr+json"))
    {
        return axum::Json(serde_json::json!({
            "name": "J-Claw Relay",
            "description": "JunoClaw sovereign coordination relay — write admission is a live view of the DAO's cw4 membership contract.",
            "contact": "https://junoclaw.xyz",
            "software": "https://github.com/Dragonmonk111/layer-sdk/tree/commonware/app/claw-relay",
            "version": env!("CARGO_PKG_VERSION"),
            "supported_nips": [1, 15, 42, 98],
            "limitation": {
                "restricted_writes": true,
                "auth_required": false,
                "payment_required": false
            }
        }))
        .into_response();
    }
    match ws {
        Some(u) => u.on_upgrade(move |sock| handle_conn(sock, app)),
        None => (
            axum::http::StatusCode::UPGRADE_REQUIRED,
            "websocket upgrade required",
        )
            .into_response(),
    }
}

async fn handle_conn(sock: WebSocket, app: Shared) {
    let (ws_tx, mut ws_rx) = sock.split();
    let (out_tx, mut out_rx) = mpsc::unbounded::<Message>();
    let out_tx2 = out_tx.clone();
    let writer = tokio::spawn(async move {
        let mut ws_tx = ws_tx;
        while let Some(m) = out_rx.next().await {
            if ws_tx.send(m).await.is_err() {
                break;
            }
        }
    });

    // NIP-42: challenge on connect. buzz-cli requires an ["AUTH",challenge]
    // before it will send any verb (RelayError::NoAuthChallenge otherwise).
    // Admission stays event-level — the auth handshake is for client
    // compatibility, not a trust gate.
    let challenge = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let _ = out_tx.unbounded_send(Message::Text(
        serde_json::json!(["AUTH", challenge]).to_string().into(),
    ));

    // subscription registry: sub_id -> filters
    let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut live = app.tx.subscribe();
    let fanout = {
        let subs = subs.clone();
        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            loop {
                match live.recv().await {
                    Ok(ev) => {
                        let map = subs.read().await;
                        let mut dead = false;
                        for (sub, filters) in map.iter() {
                            if filters.iter().any(|f| f.matches(&ev))
                                && out_tx
                                    .unbounded_send(Message::Text(
                                        serde_json::json!(["EVENT", sub, *ev])
                                            .to_string()
                                            .into(),
                                    ))
                                    .is_err()
                            {
                                dead = true;
                                break;
                            }
                        }
                        drop(map);
                        if dead {
                            return; // out_rx closed — client is gone
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // fell behind the broadcast window — tell the client
                        // to resubscribe rather than silently stall live feed
                        let _ = out_tx.unbounded_send(Message::Text(
                            serde_json::json!([
                                "NOTICE",
                                format!("error: lagged {n} events — resubscribe")
                            ])
                            .to_string()
                            .into(),
                        ));
                        return;
                    }
                    Err(_) => return,
                }
            }
        })
    };

    while let Some(Ok(msg)) = ws_rx.next().await {
        let Message::Text(txt) = msg else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            let _ = out_tx2.unbounded_send(Message::Text(
                r#"["NOTICE","error: bad json"]"#.into(),
            ));
            continue;
        };
        handle_frame(&app, &subs, &out_tx2, &v, &challenge).await;
    }
    // conn closed: abort the fan-out (it would otherwise leak on the
    // broadcast receiver forever), drop BOTH senders so out_rx closes
    // and the writer task can actually finish.
    fanout.abort();
    drop(out_tx);
    drop(out_tx2);
    let _ = writer.await;
}

async fn handle_frame(
    app: &Shared,
    subs: &Arc<RwLock<HashMap<String, Vec<Filter>>>>,
    out: &mpsc::UnboundedSender<Message>,
    v: &serde_json::Value,
    challenge: &str,
) {
    let send = |v: serde_json::Value| {
        let _ = out.unbounded_send(Message::Text(v.to_string().into()));
    };
    let Some(kind) = v.get(0).and_then(|x| x.as_str()) else {
        return send(serde_json::json!(["NOTICE", "error: unknown frame"]));
    };
    match kind {
        "EVENT" => {
            let Some(ev) = v
                .get(1)
                .and_then(|e| serde_json::from_value::<Event>(e.clone()).ok())
            else {
                return send(serde_json::json!(["NOTICE", "error: malformed event"]));
            };
            let id = ev.id.clone();
            let reply = |ok: bool, msg: &str| send(serde_json::json!(["OK", id, ok, msg]));

            if let Err(msg) = admit_event(app, &ev).await {
                return reply(false, &msg);
            }
            match app.store.put(&ev) {
                Ok(true) => {
                    reply(true, "");
                    let _ = app.tx.send(Arc::new(ev));
                }
                Ok(false) => reply(true, "duplicate: have this event"),
                Err(e) => reply(false, &format!("error: {e}")),
            }
        }
        "REQ" => {
            let Some(sub) = v.get(1).and_then(|x| x.as_str()).map(str::to_string) else {
                return send(serde_json::json!(["NOTICE", "error: malformed REQ"]));
            };
            let filters: Vec<Filter> = v
                .as_array()
                .map(|a| {
                    a.iter()
                        .skip(2)
                        .filter_map(|f| serde_json::from_value::<Filter>(f.clone()).ok())
                        .collect()
                })
                .unwrap_or_default();
            if filters.is_empty() {
                return send(serde_json::json!(["CLOSED", sub, "error: no filters"]));
            }
            // Register BEFORE replaying stored events — an event broadcast
            // between query and insert would otherwise be missed by this
            // subscriber forever. Live events may interleave with the
            // replay (legal nostr; clients dedupe by id).
            subs.write().await.insert(sub.clone(), filters.clone());
            let mut seen = HashSet::new();
            for f in &filters {
                if let Ok(events) = app.store.query(f) {
                    for ev in events {
                        if seen.insert(ev.id.clone()) {
                            send(serde_json::json!(["EVENT", sub, ev]));
                        }
                    }
                }
            }
            send(serde_json::json!(["EOSE", sub]));
        }
        "CLOSE" => {
            if let Some(sub) = v.get(1).and_then(|x| x.as_str()) {
                subs.write().await.remove(sub);
            }
        }
        "AUTH" => {
            // NIP-42 response: signed kind-22242 event carrying the issued
            // challenge. Verified for correctness (forged auth responses get
            // OK false) but not required for admission — every EVENT is
            // checked independently at admit_event.
            let Some(ev) = v
                .get(1)
                .and_then(|e| serde_json::from_value::<Event>(e.clone()).ok())
            else {
                return send(serde_json::json!(["NOTICE", "error: malformed auth event"]));
            };
            let id = ev.id.clone();
            let ok = ev.kind == 22242
                && ev.tag_value("challenge") == Some(challenge)
                && ev.verify().is_ok();
            send(serde_json::json!([
                "OK", id, ok,
                if ok { "" } else { "error: bad auth" }
            ]));
        }
        _ => send(serde_json::json!(["NOTICE", format!("error: unsupported {kind}")])),
    }
}

// ─── HTTP bridge (Buzz parity surface) ─────────────────────────────────────
// buzz-cli's durable write/read path is NOT the websocket: it POSTs to
//   /events (submit signed event)  /query (filters -> events)  /count
// each carrying NIP-98 auth: `Authorization: Nostr <base64(kind-27235 event)>`.
// The bridge and the websocket share the SAME admission gate — an event that
// would be rejected over WS is rejected here identically.
//
// Where we go past parity: writes return a structured verdict (the admit
// reason on rejection is a chain-state fact, not an opaque policy string),
// and reads may be cross-checked against moultbook checkpoint anchors.

/// NIP-98 verify: `Authorization: Nostr <b64(event)>` — kind 27235, valid
/// signature, `method`=POST, `u` tag's path matches, created_at within ±1h.
/// Returns the verified auth event; transport auth only — per-event
/// signature + allowlist admission still applies to the payload.
fn verify_nip98(
    headers: &axum::http::HeaderMap,
    method: &str,
    path: &str,
) -> Result<Event, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;
    let unauthorized = |m: &str| (StatusCode::UNAUTHORIZED, m.to_string());
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| unauthorized("missing Authorization"))?;
    let b64 = auth
        .strip_prefix("Nostr ")
        .ok_or_else(|| unauthorized("expected 'Nostr <base64-event>'"))?;
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| unauthorized("bad base64"))?;
    let ev: Event = serde_json::from_slice(&raw)
        .map_err(|_| unauthorized("auth event not valid event json"))?;
    if ev.kind != 27235 {
        return Err(unauthorized("auth event kind != 27235"));
    }
    ev.verify()
        .map_err(|_| unauthorized("bad auth event signature"))?;
    if ev.tag_value("method") != Some(method) {
        return Err(unauthorized("auth method tag mismatch"));
    }
    // u tag: authority AND path must match this request — a NIP-98
    // event is a bearer credential, so path-only compare lets an
    // event minted for any relay replay against every other relay
    // serving the same path (cross-relay replay). Bind it to the
    // authority the request actually arrived at (Host header).
    let u = ev.tag_value("u").unwrap_or_default();
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let u_ok = reqwest::Url::parse(u)
        .ok()
        .filter(|url| url.path() == path)
        .filter(|url| ["http", "https", "ws", "wss"].contains(&url.scheme()))
        .map(|url| {
            let uh = url.host_str().unwrap_or("");
            let up = url.port_or_known_default();
            // Host header is "host" or "host:port"
            let (hh, hp) = match host.rsplit_once(':') {
                Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
                    (h, p.parse::<u16>().ok())
                }
                _ => (host, None),
            };
            uh == hh && hp.map_or(true, |p| Some(p) == up)
        })
        .unwrap_or(false);
    if !u_ok {
        return Err(unauthorized("auth u tag mismatch"));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if ev.created_at.abs_diff(now) > 60 {
        return Err(unauthorized("auth event stale"));
    }
    Ok(ev)
}

fn nip98_err((code, msg): (axum::http::StatusCode, String)) -> axum::response::Response {
    (code, axum::Json(serde_json::json!({"error": msg}))).into_response()
}

/// POST /events — submit a signed event (body = raw event JSON).
/// Same admit_event gate as the WS EVENT frame.
async fn http_events(
    State(app): State<Shared>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    use axum::http::StatusCode;
    if let Err(e) = verify_nip98(&headers, "POST", "/events") {
        return nip98_err(e);
    }
    let Ok(ev) = serde_json::from_slice::<Event>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"accepted": false, "error": "malformed event"})),
        )
            .into_response();
    };
    if let Err(msg) = admit_event(&app, &ev).await {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "accepted": false, "event_id": ev.id, "error": msg,
            })),
        )
            .into_response();
    }
    match app.store.put(&ev) {
        Ok(fresh) => {
            // broadcast only on first sight — a reposted event must not
            // re-flood every live subscription
            if fresh {
                let _ = app.tx.send(std::sync::Arc::new(ev.clone()));
            }
            axum::Json(serde_json::json!({"accepted": true, "event_id": ev.id}))
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"accepted": false, "event_id": ev.id, "error": format!("{e}")})),
        )
            .into_response(),
    }
}

/// POST /query — body = JSON array of nostr filters -> JSON array of events.
async fn http_query(
    State(app): State<Shared>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    use axum::http::StatusCode;
    let auth_ev = match verify_nip98(&headers, "POST", "/query") {
        Ok(e) => e,
        Err(e) => return nip98_err(e),
    };
    // reads are member-gated too — "prove any key" is not enough on a
    // channel whose contents are private to the DAO
    if !app.allowlist.read().await.contains(&auth_ev.pubkey) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"error": "restricted: pubkey not allowlisted"})),
        )
            .into_response();
    }
    let Ok(filters) = serde_json::from_slice::<Vec<Filter>>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "filters must be a JSON array"})),
        )
            .into_response();
    };
    let mut seen = HashSet::new();
    let mut out: Vec<Event> = Vec::new();
    for f in &filters {
        if let Ok(events) = app.store.query(f) {
            for ev in events {
                if seen.insert(ev.id.clone()) {
                    out.push(ev);
                }
            }
        }
    }
    axum::Json(serde_json::json!(out)).into_response()
}

/// POST /count — body = JSON array of filters -> {"count": n}.
async fn http_count(
    State(app): State<Shared>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> axum::response::Response {
    use axum::http::StatusCode;
    let auth_ev = match verify_nip98(&headers, "POST", "/count") {
        Ok(e) => e,
        Err(e) => return nip98_err(e),
    };
    if !app.allowlist.read().await.contains(&auth_ev.pubkey) {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"error": "restricted: pubkey not allowlisted"})),
        )
            .into_response();
    }
    let Ok(filters) = serde_json::from_slice::<Vec<Filter>>(&body) else {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "filters must be a JSON array"})),
        )
            .into_response();
    };
    let mut seen = HashSet::new();
    let mut n = 0usize;
    for f in &filters {
        if let Ok(events) = app.store.query(f) {
            for ev in events {
                if seen.insert(ev.id.clone()) {
                    n += 1;
                }
            }
        }
    }
    axum::Json(serde_json::json!({"count": n})).into_response()
}

/// Shared write-admission gate — used by both client `EVENT` frames
/// and gossip ingress so a peer can't inject traffic a client couldn't
/// write directly. Order: signature, kind allowlist, h-tag channel
/// membership, pubkey allowlist.
///
/// Fail CLOSED: an empty effective allowlist denies every write
/// (never-populated cw4 feed, empty file). Anything not explicitly in
/// the set is rejected. Store dedupe happens at the caller.
async fn admit_event(app: &App, ev: &Event) -> Result<(), String> {
    ev.verify().map_err(|e| format!("invalid: {e}"))?;
    // A far-future event pins the top of every newest-first query and
    // stays unanchorable until wall-clock catches it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if ev.created_at > now + 900 {
        return Err("invalid: created_at too far in the future".into());
    }
    if !ALLOWED_KINDS.contains(&ev.kind) {
        return Err("restricted: kind not allowed".into());
    }
    let Some(chan) = ev.tag_value("h") else {
        return Err("restricted: missing channel h tag".into());
    };
    if !app.channels.is_empty() && !app.channels.contains(chan) {
        return Err("restricted: unknown channel".into());
    }
    if !app.allowlist.read().await.contains(&ev.pubkey) {
        return Err("restricted: pubkey not allowlisted".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::SigningKey;

    fn signed_event(content: &str, chan: &str, ts: u64) -> Event {
        signed_event_ex(content, Some(chan), ts, 9, &[3u8; 32])
    }

    fn signed_event_ex(
        content: &str,
        chan: Option<&str>,
        ts: u64,
        kind: u64,
        key: &[u8; 32],
    ) -> Event {
        let sk = SigningKey::from_bytes(key).unwrap();
        let tags = chan
            .map(|c| vec![vec!["h".into(), c.into()]])
            .unwrap_or_default();
        let mut e = Event {
            id: String::new(),
            pubkey: hex::encode(sk.verifying_key().to_bytes()),
            created_at: ts,
            kind,
            tags,
            content: content.into(),
            sig: String::new(),
        };
        let id = e.compute_id();
        e.id = hex::encode(id);
        e.sig = hex::encode(sk.sign_raw(&id, &[0u8; 32]).unwrap().to_bytes());
        e
    }

    #[test]
    fn store_put_query_dedupe() {
        let dir = std::env::temp_dir().join(format!("claw-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = Store::open(&dir.join("t.redb")).unwrap();
        let e = signed_event("hi", "chan-a", 100);
        assert!(st.put(&e).unwrap());
        assert!(!st.put(&e).unwrap()); // dedupe

        let f: Filter = serde_json::from_str(r##"{"#h":["chan-a"]}"##).unwrap();
        let got = st.query(&f).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].content, "hi");
        let f2: Filter = serde_json::from_str(r##"{"#h":["chan-b"]}"##).unwrap();
        assert!(st.query(&f2).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn window_ordering() {
        let dir = std::env::temp_dir().join(format!("claw-test-w-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let st = Store::open(&dir.join("t.redb")).unwrap();
        for ts in [103u64, 101, 102] {
            st.put(&signed_event(&ts.to_string(), "c", ts)).unwrap();
        }
        let win = st.channel_window("c", 101, 104).unwrap();
        assert_eq!(
            win.iter().map(|e| e.created_at).collect::<Vec<_>>(),
            vec![101, 102, 103]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Fail-CLOSED admission: an empty effective allowlist must deny
    /// every EVENT write (the old gate failed open on an empty set).
    #[tokio::test]
    async fn allowlist_fails_closed() {
        let dir = std::env::temp_dir().join(format!("claw-test-gate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = Arc::new(App {
            store: Store::open(&dir.join("t.redb")).unwrap(),
            channels: ["chan-a".to_string()].into_iter().collect(),
            allowlist: Arc::new(RwLock::new(HashSet::new())),
            tx: broadcast::channel(16).0,
        });
        let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (out_tx, mut out_rx) = mpsc::unbounded::<Message>();
        async fn next_json(rx: &mut mpsc::UnboundedReceiver<Message>) -> serde_json::Value {
            match rx.next().await.expect("reply") {
                Message::Text(t) => serde_json::from_str::<serde_json::Value>(&t).unwrap(),
                _ => panic!("non-text reply"),
            }
        }

        // 1. empty allowlist -> denied (never-populated cw4 feed)
        let ev = signed_event("hi", "chan-a", 1);
        handle_frame(&app, &subs, &out_tx, &serde_json::json!(["EVENT", ev]), "t").await;
        let v = next_json(&mut out_rx).await;
        assert_eq!(v[0], "OK");
        assert_eq!(v[2], false);
        assert!(v[3].as_str().unwrap().starts_with("restricted:"));

        // 2. populated set, pubkey not in it -> still denied
        app.allowlist.write().await.insert("f".repeat(64));
        let ev2 = signed_event("hi2", "chan-a", 2);
        handle_frame(&app, &subs, &out_tx, &serde_json::json!(["EVENT", ev2.clone()]), "t").await;
        let v = next_json(&mut out_rx).await;
        assert_eq!(v[2], false);

        // 3. allowlisted pubkey -> admitted
        app.allowlist.write().await.insert(ev2.pubkey.clone());
        handle_frame(&app, &subs, &out_tx, &serde_json::json!(["EVENT", ev2]), "t").await;
        let v = next_json(&mut out_rx).await;
        assert_eq!(v[0], "OK");
        assert_eq!(v[2], true);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The shared admission gate used by both client EVENT frames and
    /// gossip ingress: every reject path plus the accept path.
    #[tokio::test]
    async fn admit_event_accepts_and_rejects() {
        let dir =
            std::env::temp_dir().join(format!("claw-test-admit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = Arc::new(App {
            store: Store::open(&dir.join("t.redb")).unwrap(),
            channels: ["chan-a".to_string()].into_iter().collect(),
            allowlist: Arc::new(RwLock::new(HashSet::new())),
            tx: broadcast::channel(16).0,
        });

        let key = [7u8; 32];
        let good = signed_event_ex("hi", Some("chan-a"), 1, 9, &key);

        // empty allowlist -> fail closed
        assert!(admit_event(&app, &good).await.is_err());
        app.allowlist.write().await.insert(good.pubkey.clone());
        assert!(admit_event(&app, &good).await.is_ok());

        // bad signature -> invalid
        let mut bad_sig = signed_event_ex("x", Some("chan-a"), 2, 9, &key);
        bad_sig.sig = "00".repeat(64);
        let e = admit_event(&app, &bad_sig).await.unwrap_err();
        assert!(e.starts_with("invalid:"), "got: {e}");

        // kind outside ALLOWED_KINDS -> restricted (properly signed)
        let bad_kind = signed_event_ex("x", Some("chan-a"), 3, 1, &key);
        assert_eq!(
            admit_event(&app, &bad_kind).await.unwrap_err(),
            "restricted: kind not allowed"
        );

        // missing h tag -> restricted
        let no_h = signed_event_ex("x", None, 4, 9, &key);
        assert_eq!(
            admit_event(&app, &no_h).await.unwrap_err(),
            "restricted: missing channel h tag"
        );

        // h tag naming an unconfigured channel -> restricted
        let wrong_chan = signed_event_ex("x", Some("chan-b"), 5, 9, &key);
        assert_eq!(
            admit_event(&app, &wrong_chan).await.unwrap_err(),
            "restricted: unknown channel"
        );

        // unknown channel check skipped when --channels is empty
        let app_open = Arc::new(App {
            store: Store::open(&dir.join("t2.redb")).unwrap(),
            channels: HashSet::new(),
            allowlist: app.allowlist.clone(),
            tx: broadcast::channel(16).0,
        });
        let any_chan = signed_event_ex("x", Some("chan-b"), 6, 9, &key);
        assert!(admit_event(&app_open, &any_chan).await.is_ok());

        // valid sig, right channel, pubkey not allowlisted -> restricted
        let outsider = signed_event_ex("x", Some("chan-a"), 7, 9, &[9u8; 32]);
        assert_eq!(
            admit_event(&app, &outsider).await.unwrap_err(),
            "restricted: pubkey not allowlisted"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
