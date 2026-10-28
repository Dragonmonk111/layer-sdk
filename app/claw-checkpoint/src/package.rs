//! Anchor-package computation — a line-for-line port of
//! `ops/buzz/transcript-anchor.py`'s `anchor_package`, so the Rust daemon
//! produces the same windows, the same roots, and the same `to_ts`
//! boundaries as the Python ops daemon.
//!
//! Semantics preserved from the reference:
//!   1. Dedupe by `id` — the LAST event seen with a given id wins
//!      (Python `{e["id"]: e for e in events}`).
//!   2. Sort by `(created_at, id)`.
//!   3. `size_bytes` = sum of UTF-8 byte lengths of `content`.
//!   4. `--max-size` truncation keeps the largest prefix that ends on a
//!      `created_at` boundary (a whole-second prefix); `to_ts` becomes the
//!      `created_at` of the first excluded event, so the next window
//!      resumes exactly where this one stopped.
//!   5. Without truncation `to_ts` = `max(last.created_at + 1, claimed_to)`
//!      — the anchor covers quiet tail time, not just up to the last msg.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use claw_core::anchor::{leaf_hash, merkle_root, FORMAT_TAG};
use claw_core::Event;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("empty window — nothing to anchor")]
    EmptyWindow,
    #[error("message missing/invalid 64-hex id: {0:?}")]
    BadEventId(String),
    #[error("first second of window alone exceeds max-size; refusing")]
    FirstSecondTooLarge,
}

/// The anchor package — same fields the Python `compute` prints.
#[derive(Debug, Clone, Serialize)]
pub struct AnchorPackage {
    pub format: String,
    pub root_hex: String,
    pub root_b64: String,
    pub count: usize,
    pub size_bytes: u64,
    pub from_ts: u64,
    /// Exclusive upper bound of THIS anchored window — the value that
    /// becomes the next window's `next_from_ts`.
    pub to_ts: u64,
    pub first_id: String,
    pub last_id: String,
    pub truncated: bool,
}

/// Build the anchor package over a fetched window. `max_size` is the
/// contract-byte cap on committed content; `claimed_to` is the daemon's
/// requested window end (covers quiet tail time). Events need not be
/// deduped or sorted — done here, identically to the reference.
pub fn anchor_package(
    events: &[Event],
    max_size: Option<u64>,
    claimed_to: Option<u64>,
) -> Result<AnchorPackage, PackageError> {
    if events.is_empty() {
        return Err(PackageError::EmptyWindow);
    }
    for e in events {
        if e.id.len() != 64 || !e.id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(PackageError::BadEventId(e.id.clone()));
        }
    }

    // Dedupe by id (last wins — Python dict semantics), then sort.
    let mut uniq: HashMap<&str, &Event> = HashMap::with_capacity(events.len());
    for e in events {
        uniq.insert(e.id.as_str(), e);
    }
    let mut ordered: Vec<&Event> = uniq.into_values().collect();
    ordered.sort_by(|a, b| (a.created_at, a.id.as_str()).cmp(&(b.created_at, b.id.as_str())));

    // Largest prefix fitting max_size that ends on a created_at boundary.
    let mut truncated = false;
    if let Some(max) = max_size {
        let mut total: u64 = 0;
        let mut cut = 0usize;
        for (i, e) in ordered.iter().enumerate() {
            let nxt = total + e.content.len() as u64;
            if nxt > max {
                break;
            }
            total = nxt;
            if i + 1 == ordered.len() || ordered[i + 1].created_at != e.created_at {
                cut = i + 1;
            }
        }
        if cut < ordered.len() {
            truncated = true;
            if cut == 0 {
                return Err(PackageError::FirstSecondTooLarge);
            }
            ordered.truncate(cut);
        }
    }

    // Exclusive bound of this anchored window.
    let mut ordered_to = ordered.last().expect("non-empty").created_at + 1;
    if truncated {
        // bound = created_at of the first excluded event
        let kept: HashSet<&str> = ordered.iter().map(|e| e.id.as_str()).collect();
        ordered_to = events
            .iter()
            .filter(|e| !kept.contains(e.id.as_str()))
            .map(|e| e.created_at)
            .min()
            .unwrap_or(ordered_to);
    } else if let Some(c) = claimed_to {
        if c > ordered_to {
            ordered_to = c;
        }
    }

    let mut leaves = Vec::with_capacity(ordered.len());
    for e in &ordered {
        let id_bytes: [u8; 32] = hex::decode(&e.id)
            .map_err(|_| PackageError::BadEventId(e.id.clone()))?
            .as_slice()
            .try_into()
            .map_err(|_| PackageError::BadEventId(e.id.clone()))?;
        leaves.push(leaf_hash(&id_bytes));
    }
    let root = merkle_root(&leaves);
    let size_bytes: u64 = ordered.iter().map(|e| e.content.len() as u64).sum();

    Ok(AnchorPackage {
        format: FORMAT_TAG.to_string(),
        root_hex: hex::encode(root),
        root_b64: STANDARD.encode(root),
        count: ordered.len(),
        size_bytes,
        from_ts: ordered[0].created_at,
        to_ts: ordered_to,
        first_id: ordered[0].id.clone(),
        last_id: ordered.last().expect("non-empty").id.clone(),
        truncated,
    })
}

/// The canonical checkpoint payload — the exact byte string the checkpoint
/// signature commits to (BIP-340 over its SHA256). Field order is fixed by
/// declaration; serde_json emits struct fields in declaration order and
/// serde_json::to_string produces compact JSON, so this is deterministic.
#[derive(Serialize)]
struct CheckpointPayload<'a> {
    format: &'a str,
    root_hex: &'a str,
    count: usize,
    size_bytes: u64,
    from_ts: u64,
    to_ts: u64,
    first_id: &'a str,
    last_id: &'a str,
}

pub fn canonical_checkpoint_json(p: &AnchorPackage) -> String {
    serde_json::to_string(&CheckpointPayload {
        format: &p.format,
        root_hex: &p.root_hex,
        count: p.count,
        size_bytes: p.size_bytes,
        from_ts: p.from_ts,
        to_ts: p.to_ts,
        first_id: &p.first_id,
        last_id: &p.last_id,
    })
    .expect("checkpoint payload serialization cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id_byte: u8, ts: u64, content: &str) -> Event {
        Event {
            id: hex::encode([id_byte; 32]),
            pubkey: "ab".repeat(32),
            created_at: ts,
            kind: 9,
            tags: vec![vec!["h".into(), "chan".into()]],
            content: content.into(),
            sig: "cd".repeat(64),
        }
    }

    #[test]
    fn fits_without_truncation_and_claimed_to_covers_tail() {
        let events = vec![ev(1, 100, "aa"), ev(2, 200, "bb"), ev(3, 300, "cc")];
        let p = anchor_package(&events, Some(1000), Some(500)).unwrap();
        assert!(!p.truncated);
        assert_eq!(p.count, 3);
        assert_eq!(p.size_bytes, 6);
        assert_eq!(p.from_ts, 100);
        // max(last.created_at + 1, claimed_to) = max(301, 500)
        assert_eq!(p.to_ts, 500);
        assert_eq!(p.first_id, ev(1, 0, "").id);
        assert_eq!(p.last_id, ev(3, 0, "").id);
    }

    #[test]
    fn claimed_to_below_last_bound_loses() {
        let events = vec![ev(1, 100, ""), ev(2, 300, "")];
        let p = anchor_package(&events, Some(1000), Some(50)).unwrap();
        assert_eq!(p.to_ts, 301); // last.created_at + 1 wins
    }

    #[test]
    fn truncation_cuts_on_whole_second_boundary() {
        // e1@100(10B), e2@200(10B), e3@200(10B), e4@300(10B), max=20:
        // e2 fits but its same-second twin e3 does not — cut must stay
        // at the previous boundary (e1 only); to_ts = e2's second.
        let events = vec![
            ev(1, 100, &"x".repeat(10)),
            ev(2, 200, &"x".repeat(10)),
            ev(3, 200, &"x".repeat(10)),
            ev(4, 300, &"x".repeat(10)),
        ];
        let p = anchor_package(&events, Some(20), Some(999)).unwrap();
        assert!(p.truncated);
        assert_eq!(p.count, 1);
        assert_eq!(p.size_bytes, 10);
        assert_eq!(p.to_ts, 200); // first excluded second — next window resumes here
    }

    #[test]
    fn truncation_includes_completed_second() {
        // same set, max=30: e2+e3 (same second) both fit → cut at 3,
        // to_ts = e4's second (300), claimed_to must NOT apply.
        let events = vec![
            ev(1, 100, &"x".repeat(10)),
            ev(2, 200, &"x".repeat(10)),
            ev(3, 200, &"x".repeat(10)),
            ev(4, 300, &"x".repeat(10)),
        ];
        let p = anchor_package(&events, Some(30), Some(999)).unwrap();
        assert!(p.truncated);
        assert_eq!(p.count, 3);
        assert_eq!(p.size_bytes, 30);
        assert_eq!(p.to_ts, 300);
    }

    #[test]
    fn first_second_too_large_refuses() {
        let events = vec![ev(1, 100, &"x".repeat(100))];
        let e = anchor_package(&events, Some(10), None).unwrap_err();
        assert!(matches!(e, PackageError::FirstSecondTooLarge));
    }

    #[test]
    fn empty_window_refuses() {
        let e = anchor_package(&[], Some(10), None).unwrap_err();
        assert!(matches!(e, PackageError::EmptyWindow));
    }

    #[test]
    fn bad_id_refuses() {
        let mut e = ev(1, 100, "");
        e.id = "not-hex".into();
        assert!(matches!(
            anchor_package(&[e], Some(10), None).unwrap_err(),
            PackageError::BadEventId(_)
        ));
    }

    #[test]
    fn dedupe_by_id_last_wins() {
        // Python `{e["id"]: e for e in events}` — the later copy wins, so
        // size_bytes reflects the last occurrence's content.
        let mut a = ev(1, 100, "first");
        let mut b = a.clone();
        b.content = "last-copy".into();
        a.content = "first".into();
        let p = anchor_package(&[a, b], Some(1000), None).unwrap();
        assert_eq!(p.count, 1);
        assert_eq!(p.size_bytes, 9); // "last-copy"
    }

    #[test]
    fn sort_is_created_at_then_id() {
        // Same created_at: lower id sorts first.
        let hi = ev(0xff, 100, "");
        let lo = ev(0x01, 100, "");
        let later = ev(0x02, 200, "");
        let p = anchor_package(&[hi, later.clone(), lo], Some(1000), None).unwrap();
        assert_eq!(p.first_id, hex::encode([0x01u8; 32]));
        assert_eq!(p.last_id, later.id);
    }

    #[test]
    fn matches_python_reference_root() {
        // Cross-implementation vector: these are the same three event ids
        // used by claw-core's anchor test — root produced by
        // ops/buzz/transcript-anchor.py is the expected value.
        let ids = [
            "5ec8a4dc9913ae3d937f13157a433bcc25d5b9a93c1986276ae9f5cca9567b77",
            "d280c6021bf3ee823ce032134fd6200c86af19b6f36d0250fd985b3c4b5f2076",
            "cd068907ee146c8686482f543244ed7d46f27945d7b87ccbacb52833cafec9a5",
        ];
        let events: Vec<Event> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Event {
                id: id.to_string(),
                pubkey: "ab".repeat(32),
                created_at: 100 + i as u64,
                kind: 9,
                tags: vec![],
                content: String::new(),
                sig: String::new(),
            })
            .collect();
        let p = anchor_package(&events, None, None).unwrap();
        assert_eq!(
            p.root_hex,
            "957377783a77bfc76f2f7dab687422cd4646d29369e083742a80267a0260a52a"
        );
        assert_eq!(p.to_ts, 103); // last.created_at + 1, no claimed_to
    }

    #[test]
    fn canonical_checkpoint_json_is_exact() {
        let p = AnchorPackage {
            format: FORMAT_TAG.to_string(),
            root_hex: "ab".repeat(32),
            root_b64: "AAAA".into(),
            count: 3,
            size_bytes: 42,
            from_ts: 100,
            to_ts: 500,
            first_id: "11".repeat(32),
            last_id: "33".repeat(32),
            truncated: false,
        };
        assert_eq!(
            canonical_checkpoint_json(&p),
            format!(
                "{{\"format\":\"{}\",\"root_hex\":\"{}\",\"count\":3,\"size_bytes\":42,\"from_ts\":100,\"to_ts\":500,\"first_id\":\"{}\",\"last_id\":\"{}\"}}",
                FORMAT_TAG,
                "ab".repeat(32),
                "11".repeat(32),
                "33".repeat(32),
            )
        );
    }
}
