//! End-to-end: spawn the real relay binary, drive a signed Nostr session
//! over WebSocket — EVENT accepted, REQ replays it, EOSE closes the
//! window. This is the same wire path buzz-cli uses.

use claw_core::Event;
use futures_util::{SinkExt, StreamExt};
use k256::schnorr::SigningKey;
use std::net::TcpListener;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_req_eose_roundtrip() {
    let port = free_port();
    let chan = "11111111-2222-3333-4444-555555555555";
    let key = [42u8; 32];
    let pubkey_hex = hex::encode(
        SigningKey::from_bytes(&key)
            .unwrap()
            .verifying_key()
            .to_bytes(),
    );

    let dir = std::env::temp_dir().join(format!("claw-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let allow = dir.join("allow.txt");
    std::fs::write(&allow, format!("{pubkey_hex}\n")).unwrap();
    let db = dir.join("relay.redb");

    let bin = env!("CARGO_BIN_EXE_claw-relay");
    let mut child = std::process::Command::new(bin)
        .args([
            "--listen", &format!("127.0.0.1:{port}"),
            "--db", db.to_str().unwrap(),
            "--channels", chan,
            "--allowlist", allow.to_str().unwrap(),
        ])
        .spawn()
        .expect("spawn claw-relay");

    // wait for the listener
    for _ in 0..50 {
        if std::net::TcpStream::connect(format!("127.0.0.1:{port}")).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let url = format!("ws://127.0.0.1:{port}/");
    let (mut ws, _) = connect_async(&url).await.expect("ws connect");

    // 1. signed EVENT -> OK true
    let ev = sign("hello sovereign relay", chan, 1_700_000_000, &key);
    let want_id = ev.id.clone();
    ws.send(Message::Text(
        serde_json::json!(["EVENT", ev]).to_string().into(),
    ))
    .await
    .unwrap();
    let reply = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply[0], "OK");
    assert_eq!(reply[1], want_id);
    assert_eq!(reply[2], true);

    // 2. a forged event (bad sig) -> OK false
    let mut bad = sign("forged", chan, 1_700_000_001, &key);
    bad.sig = "00".repeat(64);
    let bad_id = bad.id.clone();
    ws.send(Message::Text(
        serde_json::json!(["EVENT", bad]).to_string().into(),
    ))
    .await
    .unwrap();
    let reply = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply[0], "OK");
    assert_eq!(reply[1], bad_id);
    assert_eq!(reply[2], false);

    // 3. REQ on the channel -> our event back + EOSE
    ws.send(Message::Text(
        serde_json::json!(["REQ", "s1", {"#h": [chan], "kinds": [9]}])
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    let mut got_event = false;
    let mut got_eose = false;
    for _ in 0..4 {
        let m = ws.next().await.unwrap().unwrap().into_text().unwrap();
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        match v[0].as_str().unwrap() {
            "EVENT" => {
                assert_eq!(v[1], "s1");
                assert_eq!(v[2]["id"], want_id);
                assert_eq!(v[2]["content"], "hello sovereign relay");
                got_event = true;
            }
            "EOSE" => {
                got_eose = true;
                break;
            }
            _ => {}
        }
    }
    assert!(got_event && got_eose, "missing EVENT or EOSE");

    // 4. non-allowlisted pubkey rejected
    let outsider = sign("intruder", chan, 1_700_000_002, &[9u8; 32]);
    let out_id = outsider.id.clone();
    ws.send(Message::Text(
        serde_json::json!(["EVENT", outsider]).to_string().into(),
    ))
    .await
    .unwrap();
    let reply = ws.next().await.unwrap().unwrap().into_text().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply[0], "OK");
    assert_eq!(reply[1], out_id);
    assert_eq!(reply[2], false);

    let _ = child.kill();
    std::fs::remove_dir_all(&dir).ok();
}
