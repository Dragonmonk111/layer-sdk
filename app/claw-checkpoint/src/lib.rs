//! claw-checkpoint — sovereign transcript anchoring daemon.
//!
//! Periodically Merkle-roots each claw-relay channel's event window
//! (`JC-TRANSCRIPT-ANCHOR-v1`, implemented by [`claw_core::anchor`]) and
//! anchors the root on-chain as a moultbook entry, then posts a signed
//! receipt event back to the channel. This is the Rust-native replacement
//! for `ops/buzz/transcript-anchor.sh` — the on-chain entry format and the
//! window/commitment semantics are byte-identical so anchors chain
//! correctly against entries the shell daemon produced.
//!
//! The pure logic (window packaging, paging walk, state file, canonical
//! checkpoint serialization) lives in this library so it is unit-testable
//! without a relay or a chain; `main.rs` is the thin CLI/daemon shell.

pub mod net;
pub mod package;
pub mod sign;
pub mod state;
pub mod window;

pub use net::{
    extract_txhash, find_entry_by_commitment, http_get, smart_query_url, RelayClient,
};
pub use package::{
    anchor_package, canonical_checkpoint_json, AnchorPackage, PackageError,
};
pub use sign::{load_signing_key, make_event, sign_checkpoint, CheckpointSig};
pub use state::ChannelState;
pub use window::{fetch_window, PageSource, MAX_PAGES, PAGE_LIMIT};

/// moultbook `content_type` for anchor entries — must match the shell
/// daemon exactly so verifiers see one contiguous entry format.
pub const CONTENT_TYPE: &str = "x-junoclaw/transcript-anchor";

/// The moultbook contract rejects `size_bytes` above this (v0 stores
/// `max_size_bytes = 65536`); `--max-size` must stay under it.
pub const CONTRACT_MAX_SIZE: u64 = 65_536;

/// Errors shared by every layer of the daemon.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error("websocket: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("relay: {0}")]
    Relay(String),
    #[error("http: {0}")]
    Http(String),
    #[error("timed out: {0}")]
    Timeout(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("key: {0}")]
    Key(String),
    #[error("config: {0}")]
    Config(String),
}
