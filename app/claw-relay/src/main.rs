//! claw-relay — JunoClaw sovereign coordination relay.
//!
//! Nostr wire protocol over WebSocket, wire-compatible with buzz-cli:
//!   client -> ["EVENT",{event}] | ["REQ","subid",{filter}...] | ["CLOSE","subid"]
//!   relay  -> ["OK",id,accepted,"msg"] | ["EVENT","subid",{event}]
//!             | ["EOSE","subid"] | ["NOTICE","msg"]
//!
//! Admission (MVP): writes require the event's pubkey in the allowlist
//! file AND an `h` tag naming a configured channel. Reads are open —
//! transcripts are meant to be verifiable by anyone. On-chain cw4
//! admission replaces the file in the next phase.
//!
//! Usage:
//!   claw-relay --listen 127.0.0.1:7070 --db /var/lib/claw/relay.redb \
//!     --channels <uuid1,uuid2,...> --allowlist /etc/claw/allowlist.txt

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
};
use store::Store;
use tokio::sync::{broadcast, RwLock};

/// Kinds accepted for channel writes — mirrors the set buzz-cli reads by
/// default: 9 chat, 40002/40008 channel msgs, 45001/45003 mod events.
const ALLOWED_KINDS: &[u64] = &[9, 40002, 40008, 45001, 45003];

struct App {
    store: Store,
    channels: HashSet<String>,
    allowlist: RwLock<HashSet<String>>,
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
    let allowlist = load_allowlist(allowlist_path.as_deref());

    let app = Arc::new(App {
        store: Store::open(&db)?,
        channels,
        allowlist: RwLock::new(allowlist),
        tx: broadcast::channel(4096).0,
    });

    let router = Router::new()
        .route("/", get(ws_handler))
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

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<Shared>) -> impl IntoResponse {
    ws.on_upgrade(move |sock| handle_conn(sock, app))
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

    // subscription registry: sub_id -> filters
    let subs: Arc<RwLock<HashMap<String, Vec<Filter>>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut live = app.tx.subscribe();
    {
        let subs = subs.clone();
        let out_tx = out_tx.clone();
        tokio::spawn(async move {
            while let Ok(ev) = live.recv().await {
                let map = subs.read().await;
                for (sub, filters) in map.iter() {
                    if filters.iter().any(|f| f.matches(&ev)) {
                        let _ = out_tx.unbounded_send(Message::Text(
                            serde_json::json!(["EVENT", sub, *ev]).to_string().into(),
                        ));
                    }
                }
            }
        });
    }

    while let Some(Ok(msg)) = ws_rx.next().await {
        let Message::Text(txt) = msg else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            let _ = out_tx2.unbounded_send(Message::Text(
                r#"["NOTICE","error: bad json"]"#.into(),
            ));
            continue;
        };
        handle_frame(&app, &subs, &out_tx2, &v).await;
    }
    drop(out_tx);
    let _ = writer.await;
}

async fn handle_frame(
    app: &Shared,
    subs: &Arc<RwLock<HashMap<String, Vec<Filter>>>>,
    out: &mpsc::UnboundedSender<Message>,
    v: &serde_json::Value,
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

            if let Err(e) = ev.verify() {
                return reply(false, &format!("invalid: {e}"));
            }
            if !ALLOWED_KINDS.contains(&ev.kind) {
                return reply(false, "restricted: kind not allowed");
            }
            let Some(chan) = ev.tag_value("h").map(str::to_string) else {
                return reply(false, "restricted: missing channel h tag");
            };
            if !app.channels.is_empty() && !app.channels.contains(&chan) {
                return reply(false, "restricted: unknown channel");
            }
            if !app.allowlist.read().await.is_empty()
                && !app.allowlist.read().await.contains(&ev.pubkey)
            {
                return reply(false, "restricted: pubkey not allowlisted");
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
            // serve stored events
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
            subs.write().await.insert(sub, filters);
        }
        "CLOSE" => {
            if let Some(sub) = v.get(1).and_then(|x| x.as_str()) {
                subs.write().await.remove(sub);
            }
        }
        _ => send(serde_json::json!(["NOTICE", format!("error: unsupported {kind}")])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::schnorr::SigningKey;

    fn signed_event(content: &str, chan: &str, ts: u64) -> Event {
        let sk = SigningKey::from_bytes(&[3u8; 32]).unwrap();
        let mut e = Event {
            id: String::new(),
            pubkey: hex::encode(sk.verifying_key().to_bytes()),
            created_at: ts,
            kind: 9,
            tags: vec![vec!["h".into(), chan.into()]],
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
}
