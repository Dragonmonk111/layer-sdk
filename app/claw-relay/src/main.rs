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
        ConnectInfo, DefaultBodyLimit, State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use claw_core::{Event, Filter};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use store::Store;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Kinds accepted for channel writes — mirrors the set buzz-cli reads by
/// default: 9 chat, 40002/40008 channel msgs, 45001/45003 mod events.
const ALLOWED_KINDS: &[u64] = &[9, 40002, 40008, 45001, 45003];

// ─── abuse limits (DoS tranche) ──────────────────────────────────────────
// Everything an unauthenticated or authenticated client can make the relay
// do must be bounded: frame size, event size, filter complexity, replay
// volume, subscription count, connection count, and per-key request rate.
// Rejections are logged at warn — the limiter itself bounds the log rate.
const MAX_EVENT_CONTENT: usize = 64 * 1024; // bytes of event `content`
const MAX_EVENT_TAGS: usize = 64; // tag count
const MAX_EVENT_TAG_BYTES: usize = 16 * 1024; // summed tag string bytes
const MAX_HTTP_BODY: usize = 256 * 1024;
const MAX_WS_MSG: usize = 256 * 1024;
const MAX_SUBS_PER_CONN: usize = 20;
const MAX_SUBID_LEN: usize = 64;
const MAX_FILTERS_PER_REQ: usize = 10;
const MAX_FILTER_VALUES: usize = 256; // per list (ids / authors / tag values)
const MAX_FILTER_TAG_KEYS: usize = 16;
const MAX_FILTER_VALUE_LEN: usize = 128; // chars per filter value
const MAX_REPLAY: usize = 2000; // total events replayed/returned per request
const MAX_CONN_PER_IP: usize = 16;
const MAX_TOTAL_CONNS: usize = 512;
const OUT_QUEUE: usize = 512; // bounded per-conn outgoing frames
const WS_RATE_PER_SEC: f64 = 20.0;
const WS_RATE_BURST: f64 = 40.0;
const HTTP_WRITES_PER_MIN: f64 = 30.0;
const HTTP_READS_PER_SEC: f64 = 2.0;
const MAX_HTTP_BUCKETS: usize = 4096;

/// Token bucket: refill at `rate`/sec up to `burst` capacity.
struct Bucket {
    tokens: f64,
    last: Instant,
    rate: f64,
    burst: f64,
}

impl Bucket {
    fn new(rate: f64, burst: f64) -> Self {
        Self {
            tokens: burst,
            last: Instant::now(),
            rate,
            burst,
        }
    }

    fn take(&mut self) -> bool {
        let now = Instant::now();
        self.tokens =
            (self.tokens + now.duration_since(self.last).as_secs_f64() * self.rate).min(self.burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Inbound WS connection accounting: per-IP fan-in plus a global ceiling.
/// Behind a loopback-forwarded peer door all ingress shows as 127.0.0.1,
/// so the per-IP cap must leave headroom for several live peer pulls.
#[derive(Default)]
struct ConnTable {
    by_ip: HashMap<IpAddr, usize>,
    total: usize,
}

struct App {
    store: Store,
    channels: HashSet<String>,
    /// Effective write-admission set (last-good cw4 npubs ∪ break-glass
    /// file entries). Arc'd so the cw4 poller task can share it.
    /// Empty = deny ALL writes (fail-closed).
    allowlist: Arc<RwLock<HashSet<String>>>,
    tx: broadcast::Sender<Arc<Event>>,
    conns: Mutex<ConnTable>,
    /// Per-pubkey HTTP budgets, keyed on the verified NIP-98 signer —
    /// the real client identity; ConnectInfo only ever sees the
    /// loopback/door address.
    http_write: Mutex<HashMap<String, Bucket>>,
    http_read: Mutex<HashMap<String, Bucket>>,
}

/// Decrements the per-IP/total counts when the connection task ends.
struct ConnGuard {
    app: Shared,
    ip: IpAddr,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let mut t = self.app.conns.lock().unwrap();
        t.total = t.total.saturating_sub(1);
        if let Some(n) = t.by_ip.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                t.by_ip.remove(&self.ip);
            }
        }
    }
}

/// Take one token from `key`'s bucket in `map`; `false` when the bucket
/// is empty OR the map is at capacity and the key is new (a flood of
/// distinct keys must not grow memory unboundedly).
fn take_budget(map: &Mutex<HashMap<String, Bucket>>, key: &str, rate: f64, burst: f64) -> bool {
    let mut m = map.lock().unwrap();
    if let Some(b) = m.get_mut(key) {
        return b.take();
    }
    if m.len() >= MAX_HTTP_BUCKETS {
        return false;
    }
    m.entry(key.to_string())
        .or_insert_with(|| Bucket::new(rate, burst))
        .take()
}

/// Filter complexity bound shared by REQ and the HTTP query/count path.
/// Unbounded id/author/tag lists turn a single request into an arbitrary
/// linear scan; bound every vector and string.
fn check_filters(fs: &[Filter]) -> Result<(), String> {
    for f in fs {
        let too_many = f.ids.as_ref().is_some_and(|v| v.len() > MAX_FILTER_VALUES)
            || f.authors.as_ref().is_some_and(|v| v.len() > MAX_FILTER_VALUES)
            || f.kinds.as_ref().is_some_and(|v| v.len() > MAX_FILTER_VALUES)
            || f.tag_filters.len() > MAX_FILTER_TAG_KEYS
            || f.tag_filters.values().any(|v| {
                v.len() > MAX_FILTER_VALUES
                    || v.iter().any(|s| s.len() > MAX_FILTER_VALUE_LEN)
            });
        if too_many {
            return Err("error: filter too complex".into());
        }
    }
    Ok(())
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
        conns: Mutex::new(ConnTable::default()),
        http_write: Mutex::new(HashMap::new()),
        http_read: Mutex::new(HashMap::new()),
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
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY))
        .with_state(app);

    tracing::info!(%listen, ?db, "claw-relay listening");
    axum::serve(
        tokio::net::TcpListener::bind(listen).await?,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
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
            "description": "JunoClaw sovereign coordination relay — write admission is a live view of the DAO's cw4 membership contract. HTTP endpoints require a NIP-98-signed request (kind-27235); reads additionally require an allowlisted signer.",
            "contact": "https://junoclaw.xyz",
            "software": "https://github.com/Dragonmonk111/layer-sdk/tree/commonware/app/claw-relay",
            "version": env!("CARGO_PKG_VERSION"),
            "supported_nips": [1, 15, 42, 98],
            "limitation": {
                "max_message_length": MAX_WS_MSG,
                "max_content_length": MAX_EVENT_CONTENT,
                "max_event_tags": MAX_EVENT_TAGS,
                "max_subscriptions": MAX_SUBS_PER_CONN,
                "max_subid_length": MAX_SUBID_LEN,
                "max_filters": MAX_FILTERS_PER_REQ,
                "max_limit": MAX_REPLAY,
                "restricted_writes": true,
                "auth_required": false,
                "payment_required": false
            }
        }))
        .into_response();
    }
    // Connection caps are enforced BEFORE the upgrade so an over-limit
    // client gets an HTTP rejection, not a half-open socket.
    {
        let mut t = app.conns.lock().unwrap();
        let ip_n = t.by_ip.get(&addr.ip()).copied().unwrap_or(0);
        if t.total >= MAX_TOTAL_CONNS || ip_n >= MAX_CONN_PER_IP {
            tracing::warn!(
                ip = %addr.ip(),
                per_ip = ip_n,
                total = t.total,
                "connection rejected: limit"
            );
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "connection limit",
            )
                .into_response();
        }
        t.total += 1;
        *t.by_ip.entry(addr.ip()).or_insert(0) += 1;
    }
    let guard = ConnGuard {
        app: app.clone(),
        ip: addr.ip(),
    };
    match ws {
        Some(u) => u
            .max_message_size(MAX_WS_MSG)
            .max_frame_size(MAX_WS_MSG)
            .on_upgrade(move |sock| async move {
                let _guard = guard;
                handle_conn(sock, app).await;
            }),
        None => (
            axum::http::StatusCode::UPGRADE_REQUIRED,
            "websocket upgrade required",
        )
            .into_response(),
    }
}

async fn handle_conn(sock: WebSocket, app: Shared) {
    let (ws_tx, mut ws_rx) = sock.split();
    // Bounded outgoing queue: a consumer slower than the feed must not
    // buffer the broadcast stream in memory forever. Replies pace on
    // .send(); the live fan-out uses try_send and marks the conn dead
    // on a full queue — honest close beats silent loss.
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(OUT_QUEUE);
    let out_tx2 = out_tx.clone();
    let dead = Arc::new(AtomicBool::new(false));
    let writer = tokio::spawn(async move {
        let mut ws_tx = ws_tx;
        while let Some(m) = out_rx.recv().await {
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
    let _ = out_tx.try_send(Message::Text(
        serde_json::json!(["AUTH", challenge]).to_string().into(),
    ));

    // subscription registry: sub_id -> filters
    let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut live = app.tx.subscribe();
    let fanout = {
        let subs = subs.clone();
        let out_tx = out_tx.clone();
        let dead = dead.clone();
        tokio::spawn(async move {
            loop {
                match live.recv().await {
                    Ok(ev) => {
                        let map = subs.read().await;
                        let mut gone = false;
                        for (sub, filters) in map.iter() {
                            if filters.iter().any(|f| f.matches(&ev))
                                && out_tx
                                    .try_send(Message::Text(
                                        serde_json::json!(["EVENT", sub, *ev])
                                            .to_string()
                                            .into(),
                                    ))
                                    .is_err()
                            {
                                gone = true;
                                break;
                            }
                        }
                        drop(map);
                        if gone {
                            dead.store(true, Ordering::Relaxed);
                            return; // queue full or closed — conn is done
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // fell behind the broadcast window — tell the client
                        // to resubscribe rather than silently stall live feed
                        let _ = out_tx.try_send(Message::Text(
                            serde_json::json!([
                                "NOTICE",
                                format!("error: lagged {n} events — resubscribe")
                            ])
                            .to_string()
                            .into(),
                        ));
                        dead.store(true, Ordering::Relaxed);
                        return;
                    }
                    Err(_) => return,
                }
            }
        })
    };

    // Per-connection inbound rate: every frame — text or not — costs one
    // token. An over-limit frame is dropped; the client gets at most one
    // NOTICE per second so a flood can't turn our own replies into the
    // amplification channel.
    let mut bucket = Bucket::new(WS_RATE_PER_SEC, WS_RATE_BURST);
    let mut last_notice = Instant::now() - Duration::from_secs(2);
    while let Some(Ok(msg)) = ws_rx.next().await {
        if dead.swap(false, Ordering::Relaxed) {
            break;
        }
        if !bucket.take() {
            if last_notice.elapsed() >= Duration::from_secs(1) {
                last_notice = Instant::now();
                let _ = out_tx2.try_send(Message::Text(
                    r#"["NOTICE","error: rate limited — slow down"]"#.into(),
                ));
            }
            continue;
        }
        let Message::Text(txt) = msg else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            let _ = out_tx2.try_send(Message::Text(
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
    out: &mpsc::Sender<Message>,
    v: &serde_json::Value,
    challenge: &str,
) {
    let send = |v: serde_json::Value| {
        let _ = out.try_send(Message::Text(v.to_string().into()));
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
            if sub.len() > MAX_SUBID_LEN {
                return send(serde_json::json!(["CLOSED", sub, "error: subid too long"]));
            }
            let arr = v.as_array();
            let n_filters = arr.map(|a| a.len().saturating_sub(2)).unwrap_or(0);
            if n_filters > MAX_FILTERS_PER_REQ {
                return send(serde_json::json!(["CLOSED", sub, "error: too many filters"]));
            }
            let filters: Vec<Filter> = arr
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
            if let Err(e) = check_filters(&filters) {
                return send(serde_json::json!(["CLOSED", sub, e]));
            }
            // Register BEFORE replaying stored events — an event broadcast
            // between query and insert would otherwise be missed by this
            // subscriber forever. Live events may interleave with the
            // replay (legal nostr; clients dedupe by id).
            {
                let mut map = subs.write().await;
                // Re-REQ on an existing id replaces the filters and does
                // not count against the per-conn subscription cap.
                if !map.contains_key(&sub) && map.len() >= MAX_SUBS_PER_CONN {
                    drop(map);
                    return send(serde_json::json!([
                        "CLOSED", sub, "error: too many subscriptions"
                    ]));
                }
                map.insert(sub.clone(), filters.clone());
            }
            // Replay is bounded: total events served per REQ regardless of
            // how many filters matched. .send().await paces the replay on
            // the consumer; a vanished receiver aborts the replay.
            let mut seen = HashSet::new();
            let mut sent = 0usize;
            'replay: for f in &filters {
                if let Ok(events) = app.store.query(f) {
                    for ev in events {
                        if seen.insert(ev.id.clone()) {
                            if sent >= MAX_REPLAY
                                || out
                                    .send(Message::Text(
                                        serde_json::json!(["EVENT", sub, ev])
                                            .to_string()
                                            .into(),
                                    ))
                                    .await
                                    .is_err()
                            {
                                break 'replay;
                            }
                            sent += 1;
                        }
                    }
                }
            }
            if sent >= MAX_REPLAY {
                send(serde_json::json!([
                    "NOTICE",
                    format!("replay truncated at {MAX_REPLAY} events — narrow filters")
                ]));
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
            // "localhost" only ever resolves to loopback — treat it as
            // interchangeable with 127.0.0.1 so a reflex-typed URL works.
            let nuh = if uh.eq_ignore_ascii_case("localhost") {
                "127.0.0.1"
            } else {
                uh
            };
            let nhh = if hh.eq_ignore_ascii_case("localhost") {
                "127.0.0.1"
            } else {
                hh
            };
            nuh.eq_ignore_ascii_case(nhh) && hp.map_or(true, |p| Some(p) == up)
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
    let auth_ev = match verify_nip98(&headers, "POST", "/events") {
        Ok(e) => e,
        Err(e) => return nip98_err(e),
    };
    // per-signer write budget — the NIP-98 pubkey is the real identity;
    // ConnectInfo only ever sees the loopback/door address
    if !take_budget(
        &app.http_write,
        &auth_ev.pubkey,
        HTTP_WRITES_PER_MIN / 60.0,
        HTTP_WRITES_PER_MIN,
    ) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({"accepted": false, "error": "rate limited"})),
        )
            .into_response();
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
    if !take_budget(
        &app.http_read,
        &auth_ev.pubkey,
        HTTP_READS_PER_SEC,
        HTTP_READS_PER_SEC * 30.0,
    ) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({"error": "rate limited"})),
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
    if let Err(e) = check_filters(&filters) {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    let mut seen = HashSet::new();
    let mut out: Vec<Event> = Vec::new();
    'q: for f in &filters {
        if let Ok(events) = app.store.query(f) {
            for ev in events {
                if seen.insert(ev.id.clone()) {
                    if out.len() >= MAX_REPLAY {
                        break 'q;
                    }
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
    if !take_budget(
        &app.http_read,
        &auth_ev.pubkey,
        HTTP_READS_PER_SEC,
        HTTP_READS_PER_SEC * 30.0,
    ) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            axum::Json(serde_json::json!({"error": "rate limited"})),
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
    if let Err(e) = check_filters(&filters) {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response();
    }
    let mut seen = HashSet::new();
    let mut n = 0usize;
    'c: for f in &filters {
        if let Ok(events) = app.store.query(f) {
            for ev in events {
                if seen.insert(ev.id.clone()) {
                    n += 1;
                    if n >= MAX_REPLAY {
                        break 'c;
                    }
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
    // Size bounds BEFORE the signature check — a giant event should not
    // even buy a verify. Applies equally to client writes and gossip.
    if ev.content.len() > MAX_EVENT_CONTENT {
        return Err("invalid: content too large".into());
    }
    if ev.tags.len() > MAX_EVENT_TAGS
        || ev
            .tags
            .iter()
            .flatten()
            .map(|s| s.len())
            .sum::<usize>()
            > MAX_EVENT_TAG_BYTES
    {
        return Err("invalid: too many tags".into());
    }
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
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
        });
        let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);
        async fn next_json(rx: &mut mpsc::Receiver<Message>) -> serde_json::Value {
            match rx.recv().await.expect("reply") {
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
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
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
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
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

    /// Size bounds run BEFORE signature verification — an oversized
    /// event must not even buy a verify.
    #[tokio::test]
    async fn admit_event_size_caps() {
        let dir = std::env::temp_dir().join(format!("claw-test-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = Arc::new(App {
            store: Store::open(&dir.join("t.redb")).unwrap(),
            channels: ["chan-a".to_string()].into_iter().collect(),
            allowlist: Arc::new(RwLock::new(HashSet::new())),
            tx: broadcast::channel(16).0,
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
        });
        let ev = signed_event_ex("hi", Some("chan-a"), 1, 9, &[7u8; 32]);
        app.allowlist.write().await.insert(ev.pubkey.clone());

        // content over 64 KiB -> rejected pre-verify
        let mut big = ev.clone();
        big.content = "x".repeat(MAX_EVENT_CONTENT + 1);
        big.id = hex::encode(big.compute_id());
        assert_eq!(
            admit_event(&app, &big).await.unwrap_err(),
            "invalid: content too large"
        );

        // >64 tags -> rejected pre-verify
        let mut taggy = ev.clone();
        taggy.tags = (0..MAX_EVENT_TAGS + 1)
            .map(|i| vec!["t".into(), i.to_string()])
            .collect();
        taggy.id = hex::encode(taggy.compute_id());
        assert_eq!(
            admit_event(&app, &taggy).await.unwrap_err(),
            "invalid: too many tags"
        );

        // right at the bounds -> accepted
        let mut edge = ev.clone();
        edge.content = "x".repeat(MAX_EVENT_CONTENT);
        edge.id = hex::encode(edge.compute_id());
        edge.sig = String::new(); // sig no longer valid, but size check passes -> verify error
        assert!(admit_event(&app, &edge)
            .await
            .unwrap_err()
            .starts_with("invalid:"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// REQ-path limits: subid length, filters-per-REQ count, filter
    /// complexity, and the per-conn subscription cap.
    #[tokio::test]
    async fn req_limits_enforced() {
        let dir = std::env::temp_dir().join(format!("claw-test-req-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = Arc::new(App {
            store: Store::open(&dir.join("t.redb")).unwrap(),
            channels: ["chan-a".to_string()].into_iter().collect(),
            allowlist: Arc::new(RwLock::new(HashSet::new())),
            tx: broadcast::channel(16).0,
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
        });
        let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(64);
        async fn next_json(rx: &mut mpsc::Receiver<Message>) -> serde_json::Value {
            match rx.recv().await.expect("reply") {
                Message::Text(t) => serde_json::from_str::<serde_json::Value>(&t).unwrap(),
                _ => panic!("non-text reply"),
            }
        }

        // subid > 64 chars -> CLOSED
        let long_id = "s".repeat(MAX_SUBID_LEN + 1);
        handle_frame(
            &app,
            &subs,
            &out_tx,
            &serde_json::json!(["REQ", long_id, {"kinds": [9]}]),
            "t",
        )
        .await;
        assert_eq!(next_json(&mut out_rx).await[2], "error: subid too long");

        // >10 filters -> CLOSED
        let many: Vec<serde_json::Value> = std::iter::once(serde_json::json!("REQ"))
            .chain(std::iter::once(serde_json::json!("sub1")))
            .chain((0..MAX_FILTERS_PER_REQ + 1).map(|_| serde_json::json!({"kinds": [9]})))
            .collect();
        handle_frame(&app, &subs, &out_tx, &serde_json::Value::Array(many), "t").await;
        assert_eq!(next_json(&mut out_rx).await[2], "error: too many filters");

        // over-complex filter (huge tag-value list) -> CLOSED
        let big_vals: Vec<String> = (0..MAX_FILTER_VALUES + 1).map(|i| i.to_string()).collect();
        handle_frame(
            &app,
            &subs,
            &out_tx,
            &serde_json::json!(["REQ", "sub2", {"#h": big_vals}]),
            "t",
        )
        .await;
        assert_eq!(next_json(&mut out_rx).await[2], "error: filter too complex");

        // fill to the subscription cap, then one more -> CLOSED
        for i in 0..MAX_SUBS_PER_CONN {
            handle_frame(
                &app,
                &subs,
                &out_tx,
                &serde_json::json!(["REQ", format!("s{i}"), {"kinds": [9]}]),
                "t",
            )
            .await;
            let _ = next_json(&mut out_rx).await; // EOSE (replay empty)
        }
        handle_frame(
            &app,
            &subs,
            &out_tx,
            &serde_json::json!(["REQ", "one-too-many", {"kinds": [9]}]),
            "t",
        )
        .await;
        assert_eq!(
            next_json(&mut out_rx).await[2],
            "error: too many subscriptions"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Token bucket: burst drains, sustained rate governs, refills.
    #[test]
    fn bucket_rate_limits() {
        let mut b = Bucket::new(1.0, 3.0);
        assert!(b.take() && b.take() && b.take()); // burst
        assert!(!b.take()); // empty
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(b.take()); // refilled ~1 token
    }

    /// Conn accounting: cap and release.
    #[test]
    fn conn_table_caps_and_releases() {
        let dir = std::env::temp_dir().join(format!("claw-test-conn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = Arc::new(App {
            store: Store::open(&dir.join("t.redb")).unwrap(),
            channels: HashSet::new(),
            allowlist: Arc::new(RwLock::new(HashSet::new())),
            tx: broadcast::channel(16).0,
            conns: Mutex::new(ConnTable::default()),
            http_write: Mutex::new(HashMap::new()),
            http_read: Mutex::new(HashMap::new()),
        });
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        let mut guards = Vec::new();
        for _ in 0..MAX_CONN_PER_IP {
            let mut t = app.conns.lock().unwrap();
            assert!(t.total < MAX_TOTAL_CONNS);
            t.total += 1;
            *t.by_ip.entry(ip).or_insert(0) += 1;
            drop(t);
            guards.push(ConnGuard {
                app: app.clone(),
                ip,
            });
        }
        assert_eq!(app.conns.lock().unwrap().by_ip[&ip], MAX_CONN_PER_IP);
        drop(guards);
        let t = app.conns.lock().unwrap();
        assert_eq!(t.total, 0);
        assert!(!t.by_ip.contains_key(&ip));
    }
}
