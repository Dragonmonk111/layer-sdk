//! Federation e2e: two real relay binaries. Relay B runs with
//! `--peer ws://127.0.0.1:<portA>` and pulls channel traffic from A.
//! A signed EVENT written to A must become queryable on B within a
//! few seconds — via stored replay if the event predates the gossip
//! REQ, or via live fan-out if it arrives after EOSE.

use claw_core::Event;
use futures_util::{SinkExt, StreamExt};
use k256::schnorr::SigningKey;
use std::net::TcpListener;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn sign(content: &str, chan: &str, ts: u64, key: &[u8; 32]) -> Event {
    let sk = SigningKey::from_bytes(key).unwrap();
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

fn wait_port(port: u16) {
    for _ in 0..50 {
        if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("port {port} never came up");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip_replicates_signed_event() {
    let port_a = free_port();
    let port_b = free_port();
    let chan = "fed-chan-0001";
    let key = [42u8; 32];
    let pubkey_hex = hex::encode(
        SigningKey::from_bytes(&key)
            .unwrap()
            .verifying_key()
            .to_bytes(),
    );

    let dir = std::env::temp_dir().join(format!("claw-gossip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // B applies the same admission checks to gossiped events as to
    // client writes — the signer must be in BOTH relays' allowlists.
    let allow = dir.join("allow.txt");
    std::fs::write(&allow, format!("{pubkey_hex}\n")).unwrap();

    let bin = env!("CARGO_BIN_EXE_claw-relay");
    let mut relay_a = std::process::Command::new(bin)
        .args([
            "--listen",
            &format!("127.0.0.1:{port_a}"),
            "--db",
            dir.join("a.redb").to_str().unwrap(),
            "--channels",
            chan,
            "--allowlist",
            allow.to_str().unwrap(),
        ])
        .spawn()
        .expect("spawn relay A");
    wait_port(port_a);

    let mut relay_b = std::process::Command::new(bin)
        .args([
            "--listen",
            &format!("127.0.0.1:{port_b}"),
            "--db",
            dir.join("b.redb").to_str().unwrap(),
            "--channels",
            chan,
            "--allowlist",
            allow.to_str().unwrap(),
            "--peer",
            &format!("ws://127.0.0.1:{port_a}"),
        ])
        .spawn()
        .expect("spawn relay B");
    wait_port(port_b);

    // Write a signed event to A.
    let (mut wa, _) = connect_async(format!("ws://127.0.0.1:{port_a}/"))
        .await
        .expect("ws connect A");
    // NIP-42: consume the connect-time AUTH challenge (not answered —
    // gossip/clients may ignore it; EVENT admission is per-signature).
    let _ = wa.next().await;
    let ev = sign("federated hello", chan, 1_700_000_000, &key);
    let want_id = ev.id.clone();
    wa.send(Message::Text(
        serde_json::json!(["EVENT", ev]).to_string().into(),
    ))
    .await
    .unwrap();
    let reply = wa.next().await.unwrap().unwrap().into_text().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply[0], "OK");
    assert_eq!(reply[1], want_id);
    assert_eq!(reply[2], true, "relay A rejected write: {reply}");

    // Poll REQ on B until the event shows up (gossip pull + admit +
    // store all take a beat).
    let (mut wb, _) = connect_async(format!("ws://127.0.0.1:{port_b}/"))
        .await
        .expect("ws connect B");
    let _ = wb.next().await; // consume AUTH challenge
    let mut found = false;
    'poll: for attempt in 0..40 {
        let sub = format!("probe-{attempt}");
        wb.send(Message::Text(
            serde_json::json!(["REQ", sub, {"#h": [chan], "kinds": [9]}])
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let m = match tokio::time::timeout_at(deadline, wb.next()).await {
                Ok(Some(Ok(m))) => m,
                _ => break,
            };
            let Ok(txt) = m.into_text() else { continue };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
                continue;
            };
            match v[0].as_str().unwrap_or("") {
                "EVENT" if v[1].as_str() == Some(sub.as_str()) => {
                    if v[2]["id"].as_str() == Some(want_id.as_str()) {
                        found = true;
                    }
                }
                "EOSE" if v[1].as_str() == Some(sub.as_str()) => break,
                _ => {}
            }
        }
        if found {
            break 'poll;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(found, "event written to A never appeared on B via gossip");

    let _ = relay_a.kill();
    let _ = relay_b.kill();
    std::fs::remove_dir_all(&dir).ok();
}
