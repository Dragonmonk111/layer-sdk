//! claw-core — the protocol core of the JunoClaw sovereign relay.
//!
//! Platform-agnostic: no tokio, no filesystem, no C dependencies — the same
//! code verifies events and transcript roots natively and under
//! `wasm32-unknown-unknown` (in-browser verification is a design goal).
//!
//! Three pieces:
//! - [`event`]: the NIP-01 event model — id derivation and BIP-340
//!   Schnorr signature verification.
//! - [`filter`]: Nostr subscription filters (`kinds`, `#h` channel tags,
//!   `since`/`until`, `limit`) and matching.
//! - [`anchor`]: `JC-TRANSCRIPT-ANCHOR-v1` — the Merkle construction the
//!   ops `transcript-anchor.py` daemon implements today; the relay uses it
//!   for signed checkpoints, verifiers (and browsers) reproduce it
//!   independently.
//! - [`envelope`]: versioned signature envelope for relay frames — the
//!   crypto-agility slot. Schnorr verifies today; MAYO parses today and
//!   verifies when the suite lands, without a wire break.

pub mod anchor;
pub mod envelope;
pub mod event;
pub mod filter;

pub use envelope::{EnvelopeError, FrameSig, SignedFrame};
pub use event::{Event, EventError};
pub use filter::Filter;
