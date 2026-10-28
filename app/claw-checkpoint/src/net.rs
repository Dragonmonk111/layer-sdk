//! Network I/O: the claw-relay WebSocket client (Nostr wire protocol) and
//! a minimal HTTP/1.1 GET for the chain smart-query verification — no
//! reqwest, just `TcpStream` + `Connection: close` on localhost.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use claw_core::Event;
use futures_util::{SinkExt, StreamExt};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::window::PageSource;
use crate::Error;

/// Per-REQ read deadline — a relay that stops answering mid-page must not
/// wedge the pass.
const REQ_TIMEOUT: Duration = Duration::from_secs(30);
/// Deadline for the `["OK",...]` after publishing the receipt event.
const OK_TIMEOUT: Duration = Duration::from_secs(15);

/// WebSocket client speaking the claw-relay Nostr wire protocol:
///   client → `["REQ",sub,{filter}]`, `["CLOSE",sub]`, `["EVENT",{ev}]`
///   relay  → `["EVENT",sub,{ev}]`, `["EOSE",sub]`, `["OK",id,bool,msg]`
///
/// Reads are open — no auth. One connection serves any number of pages
/// and channels; subs are closed right after EOSE so we bind only stored
/// history, never the live tail.
pub struct RelayClient {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_sub: u64,
}

impl RelayClient {
    pub async fn connect(url: &str) -> Result<Self, Error> {
        let (ws, _resp) = connect_async(url).await?;
        Ok(Self { ws, next_sub: 0 })
    }

    /// One stored-window page:
    /// `["REQ",sub,{"#h":[ch],"since":s,"until":u,"limit":n}]` →
    /// `["EVENT",sub,ev]*` → `["EOSE",sub]` → `["CLOSE",sub]`.
    async fn req_page(
        &mut self,
        channel: &str,
        since: u64,
        until: u64,
        limit: usize,
    ) -> Result<Vec<Event>, Error> {
        let sub = format!("cpw-{}", self.next_sub);
        self.next_sub += 1;
        let req = serde_json::json!(["REQ", sub, {
            "#h": [channel],
            "since": since,
            "until": until,
            "limit": limit,
        }]);
        self.ws.send(Message::Text(req.to_string())).await?;

        let mut events = Vec::new();
        let mut got_eose = false;
        let collect = async {
            while let Some(msg) = self.ws.next().await {
                let msg = msg?;
                let Message::Text(txt) = msg else { continue };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
                    continue;
                };
                // NOTICE frames carry no sub id; live events from earlier
                // subs carry a different one — both are skipped here.
                if v.get(1).and_then(|x| x.as_str()) != Some(sub.as_str()) {
                    continue;
                }
                match v.get(0).and_then(|x| x.as_str()) {
                    Some("EVENT") => {
                        if let Some(ev) = v
                            .get(2)
                            .and_then(|e| serde_json::from_value::<Event>(e.clone()).ok())
                        {
                            events.push(ev);
                        }
                    }
                    Some("EOSE") => {
                        got_eose = true;
                        break;
                    }
                    Some("CLOSED") => {
                        return Err(Error::Relay(format!("subscription {sub} closed: {v}")));
                    }
                    _ => {}
                }
            }
            Ok::<(), Error>(())
        };
        tokio::time::timeout(REQ_TIMEOUT, collect)
            .await
            .map_err(|_| Error::Timeout(format!("REQ {sub} exceeded {REQ_TIMEOUT:?}")))??;
        if !got_eose {
            return Err(Error::Relay(format!(
                "connection closed before EOSE for {sub}"
            )));
        }
        // Bind stored history only — a live tail arriving inside the
        // window would be anchored under a sub we no longer read anyway,
        // and the next pass re-fetches the true window from storage.
        let _ = self
            .ws
            .send(Message::Text(
                serde_json::json!(["CLOSE", sub]).to_string(),
            ))
            .await;
        Ok(events)
    }

    /// Publish a signed event; resolves on `["OK",id,true,...]`.
    pub async fn publish(&mut self, ev: &Event) -> Result<(), Error> {
        let frame = serde_json::json!(["EVENT", ev]);
        self.ws.send(Message::Text(frame.to_string())).await?;
        let want = ev.id.clone();
        let wait = async {
            while let Some(msg) = self.ws.next().await {
                let msg = msg?;
                let Message::Text(txt) = msg else { continue };
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
                    continue;
                };
                if v.get(0).and_then(|x| x.as_str()) != Some("OK")
                    || v.get(1).and_then(|x| x.as_str()) != Some(want.as_str())
                {
                    continue;
                }
                let ok = v.get(2).and_then(|x| x.as_bool()).unwrap_or(false);
                let m = v
                    .get(3)
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string();
                return if ok {
                    Ok(())
                } else {
                    Err(Error::Relay(format!("event rejected by relay: {m}")))
                };
            }
            Err(Error::Relay("connection closed before OK".into()))
        };
        tokio::time::timeout(OK_TIMEOUT, wait)
            .await
            .map_err(|_| Error::Timeout(format!("publish OK wait exceeded {OK_TIMEOUT:?}")))?
    }
}

impl PageSource for RelayClient {
    fn fetch_page<'a>(
        &'a mut self,
        channel: &'a str,
        since: u64,
        until: u64,
        limit: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Event>, Error>> + Send + 'a>> {
        Box::pin(self.req_page(channel, since, until, limit))
    }
}

/// Minimal HTTP/1.1 GET — `Connection: close`, optional chunked decoding.
/// Exists to avoid pulling reqwest for one localhost smart query.
pub async fn http_get(url: &str) -> Result<String, Error> {
    let rest = url.strip_prefix("http://").ok_or_else(|| {
        Error::Config(format!("smart-query endpoint must be http:// — got {url:?}"))
    })?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a.to_string(), format!("/{p}")),
        None => (rest.to_string(), "/".to_string()),
    };
    let addr = if authority.contains(':') {
        authority.clone()
    } else {
        format!("{authority}:80")
    };
    let mut stream = TcpStream::connect(addr.as_str()).await?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    let sep = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| Error::Http("malformed response: no header/body separator".into()))?;
    let head = String::from_utf8_lossy(&buf[..sep]);
    let status = head.lines().next().unwrap_or("").to_string();
    let code: u32 = status
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    if !(200..300).contains(&code) {
        let preview = String::from_utf8_lossy(&buf[sep + 4..]);
        return Err(Error::Http(format!("{status}: {:.200}", preview)));
    }
    let raw_body = &buf[sep + 4..];
    let lower = head.to_lowercase();
    let body = if lower.contains("transfer-encoding:") && lower.contains("chunked") {
        dechunk(raw_body)?
    } else {
        raw_body.to_vec()
    };
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Decode an HTTP/1.1 chunked body.
fn dechunk(mut buf: &[u8]) -> Result<Vec<u8>, Error> {
    let bad = || Error::Http("malformed chunked body".into());
    let mut out = Vec::new();
    loop {
        let line_end = buf.windows(2).position(|w| w == b"\r\n").ok_or_else(bad)?;
        let size_str = std::str::from_utf8(&buf[..line_end]).map_err(|_| bad())?;
        let size_str = size_str.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| bad())?;
        buf = &buf[line_end + 2..];
        if size == 0 {
            break;
        }
        if buf.len() < size {
            return Err(bad());
        }
        out.extend_from_slice(&buf[..size]);
        buf = &buf[size..];
        if buf.starts_with(b"\r\n") {
            buf = &buf[2..];
        }
    }
    Ok(out)
}

/// `GET <api>/api/wasm/smart/<contract>?query=<b64url(query)>` — same
/// endpoint + b64url-no-pad encoding the shell daemon's curl used.
pub fn smart_query_url(api: &str, contract: &str, query: &serde_json::Value) -> String {
    let q = URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(query).expect("query serialization cannot fail"),
    );
    format!(
        "{}/api/wasm/smart/{}?query={}",
        api.trim_end_matches('/'),
        contract,
        q
    )
}

/// Find OUR entry in a moultbook `list_by_author` response by commitment
/// — the committed-state check: an anchor is only real once it reads back
/// from contract state. Returns the entry id (string or number coerced).
pub fn find_entry_by_commitment(body: &str, commitment_b64: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let entries = v.get("data")?.get("entries")?.as_array()?;
    entries
        .iter()
        .find(|e| e.get("commitment").and_then(|c| c.as_str()) == Some(commitment_b64))
        .and_then(|e| e.get("id"))
        .map(|id| match id {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
}

/// First run of ≥64 uppercase-hex chars — the script's
/// `grep -oE '[0-9A-F]{64}' | head -1` over bls-relayer output.
pub fn extract_txhash(output: &str) -> Option<String> {
    let mut cur = String::new();
    for c in output.chars() {
        if c.is_ascii_digit() || ('A'..='F').contains(&c) {
            cur.push(c);
            if cur.len() == 64 {
                return Some(cur);
            }
        } else {
            cur.clear();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txhash_extraction_matches_shell_grep() {
        let out = "jc-execute juno1abc broadcast OK — txhash \
                   0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";
        assert_eq!(
            extract_txhash(out),
            Some("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF".into())
        );
        // lowercase hex does not match [0-9A-F]{64}
        assert!(extract_txhash("txhash deadbeef").is_none());
        assert!(extract_txhash("no hash at all").is_none());
    }

    #[test]
    fn smart_query_url_encodes_b64url_nopad() {
        let url = smart_query_url(
            "http://127.0.0.1:8082/",
            "juno1contract",
            &serde_json::json!({"list_by_author": {"author": "juno1seat", "limit": 20}}),
        );
        let q = url.split("query=").nth(1).unwrap();
        assert!(!q.contains('='), "padding must be stripped");
        assert!(!q.contains('+') && !q.contains('/'), "must be url-safe");
        assert!(url.starts_with("http://127.0.0.1:8082/api/wasm/smart/juno1contract?query="));
        // decodes back to the exact query JSON
        let raw = URL_SAFE_NO_PAD.decode(q).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&raw).unwrap(),
            serde_json::json!({"list_by_author": {"author": "juno1seat", "limit": 20}})
        );
    }

    #[test]
    fn finds_our_entry_by_commitment() {
        let body = r#"{"data":{"entries":[
            {"id":"41","commitment":"OTHER","author":"juno1x"},
            {"id":"42","commitment":"ROOTB64","author":"juno1x"}
        ]}}"#;
        assert_eq!(find_entry_by_commitment(body, "ROOTB64"), Some("42".into()));
        assert_eq!(find_entry_by_commitment(body, "MISSING"), None);
        // numeric ids coerce too
        let body2 = r#"{"data":{"entries":[{"id":7,"commitment":"C"}]}}"#;
        assert_eq!(find_entry_by_commitment(body2, "C"), Some("7".into()));
        assert_eq!(find_entry_by_commitment("not json", "C"), None);
    }

    #[test]
    fn dechunk_basic() {
        let body = b"4\r\nWiki\r\n6\r\npedia \r\nE\r\nin \r\n\r\nchunks.\r\n0\r\n\r\n";
        assert_eq!(
            String::from_utf8(dechunk(body).unwrap()).unwrap(),
            "Wikipedia in \r\n\r\nchunks."
        );
    }

    // ---- live tests: require a running claw-relay / chain -------------

    #[tokio::test]
    #[ignore = "requires live claw-relay on 127.0.0.1:7070"]
    async fn live_req_eose_roundtrip() {
        let mut c = RelayClient::connect("ws://127.0.0.1:7070").await.unwrap();
        // reads are open; an unknown channel still gets EOSE
        let page = c
            .req_page("nonexistent-channel", 0, u64::MAX, 10)
            .await
            .unwrap();
        assert!(page.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live REST API on 127.0.0.1:8082"]
    async fn live_http_get() {
        let body = http_get("http://127.0.0.1:8082/").await;
        assert!(body.is_ok());
    }
}
