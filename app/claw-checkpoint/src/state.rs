//! Per-channel cursor state — same file shape as the shell daemon's
//! `anchor-<name>.json`: `{"next_from_ts":N,"last_entry":"..."|null}`.
//!
//! `next_from_ts` is the start of the NEXT clock window — set to the
//! anchored window's exclusive `to_ts` after committed-state verification
//! (or to the pass end on an empty window), so ranges stay contiguous and
//! verifiable. `last_entry` is the previous on-chain moultbook entry id —
//! the `refs` link in the chain.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelState {
    pub next_from_ts: u64,
    #[serde(default)]
    pub last_entry: Option<String>,
}

impl ChannelState {
    pub fn path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("anchor-{name}.json"))
    }

    /// `Ok(None)` when no state file exists (first run — caller bootstraps
    /// `next_from_ts` from `--bootstrap-hours`).
    pub fn load(dir: &Path, name: &str) -> io::Result<Option<Self>> {
        let p = Self::path(dir, name);
        match std::fs::read_to_string(&p) {
            Ok(s) => serde_json::from_str(&s)
                .map(Some)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", p.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, dir: &Path, name: &str) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        std::fs::write(Self::path(dir, name), json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_shape() {
        let dir = std::env::temp_dir().join(format!("claw-cp-state-{}", std::process::id()));
        let st = ChannelState {
            next_from_ts: 1_700_000_000,
            last_entry: Some("42".into()),
        };
        st.save(&dir, "governance").unwrap();
        // same shape the shell daemon wrote: python json.dump of the dict
        let raw = std::fs::read_to_string(ChannelState::path(&dir, "governance")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["next_from_ts"], 1_700_000_000u64);
        assert_eq!(v["last_entry"], "42");
        let back = ChannelState::load(&dir, "governance").unwrap().unwrap();
        assert_eq!(back.next_from_ts, st.next_from_ts);
        assert_eq!(back.last_entry, st.last_entry);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_none() {
        let dir = std::env::temp_dir().join(format!("claw-cp-none-{}", std::process::id()));
        assert!(ChannelState::load(&dir, "nope").unwrap().is_none());
    }

    #[test]
    fn shell_daemon_file_parses() {
        // File as written by python json.dump in transcript-anchor.sh —
        // null last_entry on an empty-window advance.
        let dir = std::env::temp_dir().join(format!("claw-cp-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            ChannelState::path(&dir, "agents"),
            r#"{"next_from_ts": 1760000000, "last_entry": null}"#,
        )
        .unwrap();
        let st = ChannelState::load(&dir, "agents").unwrap().unwrap();
        assert_eq!(st.next_from_ts, 1_760_000_000);
        assert_eq!(st.last_entry, None);
        std::fs::remove_dir_all(&dir).ok();
    }
}
