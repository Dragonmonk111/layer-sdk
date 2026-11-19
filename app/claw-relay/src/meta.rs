//! Relay-authored NIP-29 group metadata (kinds 39000/39002) + kind-0.
//!
//! Buzz-class adapters discover channels and membership through
//! ordinary Nostr queries: kind-39000 group docs and kind-39002 member
//! lists keyed by `#d`, member kind-0 profiles by `authors`. The first
//! two are relay state, not user writes — they are synthesized here,
//! signed by the relay identity (`--identity-key`), and inserted into
//! the store directly so they bypass write admission (which requires
//! an `h` tag and an admitted author).
//!
//! Member kind-0 profiles are NOT synthesized — the relay cannot sign
//! as a member. Admitted members publish their own kind-0 instead
//! (kind 0 is in ALLOWED_KINDS and exempt from the channel-tag rule);
//! display names in the meantime ride in 39002 `p`-tag entries via
//! `--profile <npub>=<name>`.
//!
//! `created_at` is truncated to the hour: re-synthesizing an unchanged
//! roster reproduces the same event ids and store-dedupes instead of
//! accumulating near-identical events. `limit:1` + newest-first means
//! a changed roster always wins its query slot.

use claw_core::Event;
use k256::schnorr::SigningKey;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

/// The relay's secp256k1 identity — advertised as NIP-11 `pubkey` and
/// used to sign every synthesized metadata event.
pub struct RelayIdentity {
    pub npub: String,
    key: SigningKey,
}

/// Load a 64-hex secp256k1 secret file → identity. Returns an error
/// string so the caller can warn-and-degrade (no NIP-11 pubkey, no
/// metadata) rather than abort startup on a missing file.
pub fn load_identity(path: &Path) -> Result<RelayIdentity, String> {
    let raw =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let bytes = hex::decode(raw.trim())
        .map_err(|_| "identity key must be 64 hex chars".to_string())?;
    let key = SigningKey::from_bytes(
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| "identity key must be exactly 32 bytes")?,
    )
    .map_err(|e| format!("bad secp256k1 secret: {e}"))?;
    Ok(RelayIdentity {
        npub: hex::encode(key.verifying_key().to_bytes()),
        key,
    })
}

/// Build a fully-signed event under the relay identity.
fn sign_event(
    key: &SigningKey,
    created_at: u64,
    kind: u64,
    tags: Vec<Vec<String>>,
    content: String,
) -> Event {
    let mut e = Event {
        id: String::new(),
        pubkey: hex::encode(key.verifying_key().to_bytes()),
        created_at,
        kind,
        tags,
        content,
        sig: String::new(),
    };
    let id = e.compute_id();
    e.id = hex::encode(id);
    e.sig = hex::encode(key.sign_raw(&id, &[0u8; 32]).unwrap().to_bytes());
    e
}

/// The complete synthesized metadata set for the current roster:
/// - 39000 group doc per channel (`#d` + `#h` tags; `#h` keeps it
///   discoverable to channel-scoped readers and future gossip pulls)
/// - 39002 member list per channel (`#d` + `p` tags, optional name)
/// - kind-0 profile for the relay identity itself
pub fn synthesize(
    channels: &HashSet<String>,
    members: &HashSet<String>,
    channel_names: &HashMap<String, String>,
    profiles: &HashMap<String, String>,
    identity: &RelayIdentity,
    now: u64,
) -> Vec<Event> {
    let ts = now - (now % 3600);
    let mut chans: Vec<&String> = channels.iter().collect();
    chans.sort();
    let mut mems: Vec<&String> = members.iter().collect();
    mems.sort();

    let mut out = Vec::with_capacity(chans.len() * 2 + 1);
    for ch in chans {
        let name = channel_names
            .get(ch)
            .cloned()
            .unwrap_or_else(|| ch[..8.min(ch.len())].to_string());
        out.push(sign_event(
            &identity.key,
            ts,
            39000,
            vec![
                vec!["d".into(), ch.clone()],
                vec!["h".into(), ch.clone()],
                vec!["name".into(), name.clone()],
            ],
            serde_json::json!({
                "name": name,
                "about": "JunoClaw sovereign coordination room",
                "picture": ""
            })
            .to_string(),
        ));

        let mut mtags = vec![
            vec!["d".into(), ch.clone()],
            vec!["h".into(), ch.clone()],
        ];
        for m in &mems {
            let mut p = vec!["p".into(), (*m).clone()];
            if let Some(display) = profiles.get(*m) {
                p.push(display.clone());
            }
            mtags.push(p);
        }
        out.push(sign_event(
            &identity.key,
            ts,
            39002,
            mtags,
            serde_json::json!({"description": "admitted member set"}).to_string(),
        ));
    }

    out.push(sign_event(
        &identity.key,
        ts,
        0,
        vec![],
        serde_json::json!({
            "name": "J-Claw Relay",
            "about": "JunoClaw sovereign coordination relay"
        })
        .to_string(),
    ));
    out
}

/// Re-synthesize metadata whenever the effective admitted set changes,
/// on the same cadence as the admission poller. First tick always
/// synthesizes — this covers boot.
pub async fn sync_loop(
    app: crate::Shared,
    identity: RelayIdentity,
    channel_names: HashMap<String, String>,
    profiles: HashMap<String, String>,
    period: Duration,
) {
    let mut last: Option<Vec<String>> = None;
    loop {
        let mut members: Vec<String> = app.allowlist.read().await.iter().cloned().collect();
        members.sort();
        if last.as_ref() != Some(&members) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let set: HashSet<String> = members.iter().cloned().collect();
            let events = synthesize(&app.channels, &set, &channel_names, &profiles, &identity, now);
            let mut inserted = 0usize;
            for ev in events {
                match app.store.put(&ev) {
                    Ok(true) => {
                        inserted += 1;
                        let _ = app.tx.send(Arc::new(ev));
                    }
                    Ok(false) => {}
                    Err(e) => tracing::warn!(error = %e, "metadata event put failed"),
                }
            }
            tracing::info!(
                members = members.len(),
                inserted,
                relay_npub = %identity.npub,
                "nip-29 metadata synchronized"
            );
            last = Some(members);
        }
        tokio::time::sleep(period).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use claw_core::Filter;

    fn identity() -> RelayIdentity {
        RelayIdentity {
            npub: String::new(),
            key: SigningKey::from_bytes(&[3u8; 32]).unwrap(),
        }
    }

    fn setup() -> (
        HashSet<String>,
        HashSet<String>,
        HashMap<String, String>,
        HashMap<String, String>,
    ) {
        let channels: HashSet<String> =
            ["chan-a".to_string(), "chan-b".to_string()].into_iter().collect();
        let members: HashSet<String> =
            ["aa".repeat(32), "bb".repeat(32)].into_iter().collect();
        let names: HashMap<String, String> =
            [("chan-a".to_string(), "ops-room".to_string())].into_iter().collect();
        let profiles: HashMap<String, String> =
            [("aa".repeat(32), "ravi-agent".to_string())].into_iter().collect();
        (channels, members, names, profiles)
    }

    #[test]
    fn synthesized_events_verify() {
        let (channels, members, names, profiles) = setup();
        let id = identity();
        let events = synthesize(&channels, &members, &names, &profiles, &id, 3_600_000);
        // 2 channels × (39000 + 39002) + relay kind-0
        assert_eq!(events.len(), 5);
        for e in &events {
            e.verify().unwrap();
            assert_eq!(e.pubkey, hex::encode(id.key.verifying_key().to_bytes()));
            // hour-truncated timestamp
            assert_eq!(e.created_at % 3600, 0);
        }
    }

    #[test]
    fn unchanged_roster_reproduces_ids() {
        let (channels, members, names, profiles) = setup();
        let id = identity();
        let a = synthesize(&channels, &members, &names, &profiles, &id, 3_600_000);
        let b = synthesize(&channels, &members, &names, &profiles, &id, 3_600_001);
        let ids_a: HashSet<&str> = a.iter().map(|e| e.id.as_str()).collect();
        let ids_b: HashSet<&str> = b.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids_a, ids_b);
    }

    #[test]
    fn d_tag_filters_match() {
        let (channels, members, names, profiles) = setup();
        let id = identity();
        let events = synthesize(&channels, &members, &names, &profiles, &id, 3_600_000);
        let f_chan: Filter =
            serde_json::from_str(r##"{"#d":["chan-a"],"kinds":[39000]}"##).unwrap();
        let got: Vec<&Event> = events.iter().filter(|e| f_chan.matches(e)).collect();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].tag_value("name").unwrap(), "ops-room");

        let f_mem: Filter =
            serde_json::from_str(r##"{"#d":["chan-b"],"kinds":[39002]}"##).unwrap();
        let got: Vec<&Event> = events.iter().filter(|e| f_mem.matches(e)).collect();
        assert_eq!(got.len(), 1);
        assert!(got[0]
            .tags
            .iter()
            .any(|t| t.first().map(String::as_str) == Some("p")
                && t.get(1).map(String::as_str) == Some("aa".repeat(32).as_str())));
    }
}
