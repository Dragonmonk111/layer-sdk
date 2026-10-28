//! Pull-only federation ("gossip") between claw-relay instances.
//!
//! Each `--peer <ws-url>` spawns a `run_peer` task that:
//!   1. connects to `<peer>/` via tokio-tungstenite;
//!   2. sends `["REQ","gossip-<chan>",{"#h":["<chan>"],"kinds":[...]}]`
//!      for every configured channel — the peer serves its stored
//!      events, sends EOSE, and keeps the sub live for the broadcast
//!      fan-out;
//!   3. for every `["EVENT",<sub>,{ev}]` received: verify the
//!      signature, run the SAME admission checks as client writes
//!      (`admit_event`), `store.put`, and — only when the event is new
//!      — publish to `app.tx` so local subscribers see remote traffic.
//!
//! Loop safety is free: relays pull, never push, and `store.put`
//! dedupes by id at each hop, so an event that has already crossed a
//! relay is never re-ingested. Each relay independently verifies
//! signatures + admission — a peer cannot inject traffic a local
//! client couldn't write.
//!
//! Reconnect: on any session end (clean close or error) the task
//! backs off starting at 5s, doubling to a 60s cap; a successful
//! connect resets the backoff.

use crate::{admit_event, Shared, ALLOWED_KINDS};
use claw_core::Event;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream,
};

const BACKOFF_INIT: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

type PeerWs = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Persistent outbound peer task — connect, subscribe, ingest,
/// reconnect forever.
pub async fn run_peer(app: Shared, peer: String) {
    let url = format!("{}/", peer.trim_end_matches('/'));
    let mut backoff = BACKOFF_INIT;
    // first-event-per-peer is logged once for the life of this task,
    // not once per reconnect — reconnects get their own info line.
    let mut first_event_logged = false;
    loop {
        match connect_async(&url).await {
            Ok((ws, _)) => {
                tracing::info!(%peer, "gossip connected");
                backoff = BACKOFF_INIT;
                match peer_session(&app, &peer, ws, &mut first_event_logged).await {
                    Ok(()) => tracing::info!(%peer, "gossip peer closed connection"),
                    Err(e) => tracing::warn!(%peer, error = %e, "gossip session failed"),
                }
            }
            Err(e) => tracing::warn!(%peer, error = %e, "gossip connect failed"),
        }
        tracing::info!(%peer, ?backoff, "gossip reconnecting after backoff");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// One connected session: send the per-channel REQs, then ingest
/// frames until the socket closes or errors.
async fn peer_session(
    app: &Shared,
    peer: &str,
    mut ws: PeerWs,
    first_event_logged: &mut bool,
) -> Result<(), String> {
    for chan in &app.channels {
        let req = serde_json::json!([
            "REQ",
            format!("gossip-{chan}"),
            {"#h": [chan], "kinds": ALLOWED_KINDS},
        ]);
        ws.send(Message::Text(req.to_string().into()))
            .await
            .map_err(|e| format!("send REQ for {chan}: {e}"))?;
    }

    while let Some(msg) = ws.next().await {
        let msg = msg.map_err(|e| format!("ws recv: {e}"))?;
        let Message::Text(txt) = msg else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
            continue;
        };
        let Some(kind) = v.get(0).and_then(|x| x.as_str()) else {
            continue;
        };
        match kind {
            "EVENT" => handle_remote_frame(app, peer, &v, first_event_logged).await,
            // EOSE only ends the stored replay — the peer keeps the sub
            // registered and streams live events; keep listening.
            "EOSE" => tracing::debug!(%peer, "gossip EOSE, sub stays live"),
            "CLOSED" | "NOTICE" => {
                tracing::warn!(%peer, frame = %txt, "gossip peer message")
            }
            _ => {}
        }
    }
    Ok(())
}

/// `["EVENT", <sub_id>, {event}]` — parse the payload and hand it to
/// the admission+store path.
async fn handle_remote_frame(
    app: &Shared,
    peer: &str,
    v: &serde_json::Value,
    first_event_logged: &mut bool,
) {
    let Some(ev) = v
        .get(2)
        .and_then(|e| serde_json::from_value::<Event>(e.clone()).ok())
    else {
        tracing::debug!(%peer, "gossip: malformed EVENT frame dropped");
        return;
    };
    handle_remote(app, peer, ev, first_event_logged).await;
}

/// Gossip ingress for one remote event: signature + admission identical
/// to client writes, then store + live fan-out. Duplicates are dropped
/// silently — id-dedupe is the federation loop guard.
async fn handle_remote(
    app: &Shared,
    peer: &str,
    ev: Event,
    first_event_logged: &mut bool,
) {
    if let Err(reason) = admit_event(app, &ev).await {
        tracing::debug!(%peer, id = %ev.id, %reason, "gossip event rejected");
        return;
    }
    match app.store.put(&ev) {
        Ok(true) => {
            if !*first_event_logged {
                tracing::info!(%peer, id = %ev.id, "gossip: first event accepted from peer");
                *first_event_logged = true;
            } else {
                tracing::debug!(%peer, id = %ev.id, "gossip event accepted");
            }
            let _ = app.tx.send(Arc::new(ev));
        }
        Ok(false) => {} // duplicate — we already have this event
        Err(e) => tracing::warn!(%peer, error = %e, "gossip store put failed"),
    }
}
