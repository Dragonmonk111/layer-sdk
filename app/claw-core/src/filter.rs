//! Nostr subscription filters and matching (REQ semantics).
//!
//! JSON shape mirrors the NIP-01 filter: scalar fields plus `#<letter>`
//! tag filters. `{ "#h": ["chan-uuid"] }` selects events carrying that
//! channel tag — the NIP-29 convention Buzz uses and we keep, so existing
//! tooling is wire-compatible.

use crate::event::Event;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    #[serde(default)]
    pub kinds: Option<Vec<u64>>,
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub until: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
    /// Tag filters: `#h`, `#e`, `#p`, `#d`, … — JSON keys literally start
    /// with `#`; serde_json hands them through `flatten`.
    #[serde(flatten)]
    pub tag_filters: BTreeMap<String, Vec<String>>,
}

impl Filter {
    /// Tag-filter key for a letter, e.g. `tag_key("h")` -> `"#h"`.
    pub fn tag_key(letter: &str) -> String {
        format!("#{letter}")
    }

    /// All values requested for tag `letter` (e.g. `#h` channel ids).
    pub fn tag_values(&self, letter: &str) -> &[String] {
        self.tag_filters
            .get(&Self::tag_key(letter))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// `since`/`until` are INCLUSIVE bounds on `created_at` — this matches
    /// both the NIP-01 convention and how the transcript-anchor windows
    /// are verified (`--since from --before to-1` for `[from, to)`).
    pub fn matches(&self, e: &Event) -> bool {
        if let Some(ids) = &self.ids {
            if !ids.iter().any(|i| e.id.starts_with(i.as_str())) {
                return false;
            }
        }
        if let Some(authors) = &self.authors {
            if !authors.iter().any(|a| e.pubkey.starts_with(a.as_str())) {
                return false;
            }
        }
        if let Some(kinds) = &self.kinds {
            if !kinds.contains(&e.kind) {
                return false;
            }
        }
        if let Some(s) = self.since {
            if e.created_at < s {
                return false;
            }
        }
        if let Some(u) = self.until {
            if e.created_at > u {
                return false;
            }
        }
        for (key, want) in &self.tag_filters {
            let Some(letter) = key.strip_prefix('#') else {
                continue; // not a tag filter key
            };
            let has = e.tags.iter().any(|t| {
                t.first().map(String::as_str) == Some(letter)
                    && t.get(1)
                        .map(|v| want.iter().any(|w| w == v))
                        .unwrap_or(false)
            });
            if !has {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: u64, ts: u64, h: &str) -> Event {
        Event {
            id: "aa".repeat(32),
            pubkey: "bb".repeat(32),
            created_at: ts,
            kind,
            tags: vec![vec!["h".into(), h.into()]],
            content: String::new(),
            sig: String::new(),
        }
    }

    #[test]
    fn parses_tag_filters() {
        let f: Filter = serde_json::from_str(
            r##"{"kinds":[9],"#h":["chan"],"since":10,"until":20,"limit":50}"##,
        )
        .unwrap();
        assert_eq!(f.tag_values("h"), &["chan".to_string()]);
        assert_eq!(f.since, Some(10));
    }

    #[test]
    fn matches_window_and_channel() {
        let e = ev(9, 15, "chan");
        let f: Filter =
            serde_json::from_str(r##"{"kinds":[9],"#h":["chan"],"since":10,"until":20}"##).unwrap();
        assert!(f.matches(&e));
        let other_chan: Filter =
            serde_json::from_str(r##"{"kinds":[9],"#h":["other"]}"##).unwrap();
        assert!(!other_chan.matches(&e));
    }

    #[test]
    fn bounds_are_inclusive() {
        let e = ev(9, 20, "chan");
        let f: Filter = serde_json::from_str(r#"{"until":20}"#).unwrap();
        assert!(f.matches(&e));
        let f2: Filter = serde_json::from_str(r#"{"since":21}"#).unwrap();
        assert!(!f2.matches(&e));
    }
}
