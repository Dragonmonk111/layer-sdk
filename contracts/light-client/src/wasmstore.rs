//! 08-wasm host-key storage layer.
//!
//! The `08-wasm` module shares the client-prefixed KV store with this contract
//! and, after every `instantiate`/`sudo` call, reads back the client state at
//! the fixed host key `clientState` (`validatePostExecutionClientState`) and
//! consensus states at `consensusStates/{revision_number}-{revision_height}`.
//! Each value is a protobuf `google.protobuf.Any` wrapping the
//! `ibc.lightclients.wasm.v1` `ClientState`/`ConsensusState` message, whose
//! opaque `data` field carries this contract's own JSON-serialized state.
//!
//! Layout written by this contract:
//!   "clientState"              -> Any{"/ibc.lightclients.wasm.v1.ClientState",
//!                                     ClientState{data=json(ClientState),
//!                                                   checksum, latest_height}}
//!   "consensusStates/0-<h>"    -> Any{"/ibc.lightclients.wasm.v1.ConsensusState",
//!                                     ConsensusState{data=json(ConsensusState)}}
//!
//! The host only validates the `clientState` key (presence, type, checksum);
//! it never inspects `data`, so the contract's JSON lives inside it.

use cosmwasm_std::{from_json, to_json_vec, StdError, StdResult, Storage};

use crate::state::{ClientState, ConsensusState, Height};

const CLIENT_STATE_KEY: &[u8] = b"clientState";
const CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.wasm.v1.ClientState";
const CONSENSUS_STATE_TYPE_URL: &str = "/ibc.lightclients.wasm.v1.ConsensusState";

/// `consensusStates/{revision_number}-{revision_height}` — ibc-go
/// `host.ConsensusStateKey` (`Height.String()` is `{rev}-{height}`).
fn consensus_state_key(h: &Height) -> Vec<u8> {
    format!("consensusStates/{}-{}", h.revision_number, h.revision_height).into_bytes()
}

// ---------------------------------------------------------------------------
// Minimal proto3 codec (only what the wasm wrapper messages need).
// ---------------------------------------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

/// Length-delimited field (wiretype 2): tag, varint len, bytes.
fn put_len(out: &mut Vec<u8>, field: u32, data: &[u8]) {
    put_varint(out, ((field as u64) << 3) | 2);
    put_varint(out, data.len() as u64);
    out.extend_from_slice(data);
}

/// Varint field (wiretype 0). Zero values are omitted (proto3 canonical).
fn put_varint_field(out: &mut Vec<u8>, field: u32, v: u64) {
    if v != 0 {
        put_varint(out, (field as u64) << 3);
        put_varint(out, v);
    }
}

fn read_varint(bz: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    let mut shift = 0u32;
    while i < bz.len() {
        let b = bz[i];
        i += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((v, i));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Extract the bytes of a length-delimited field by field number.
fn get_len_field<'a>(bz: &'a [u8], want: u32) -> Option<&'a [u8]> {
    let mut i = 0usize;
    while i < bz.len() {
        let (tag, ni) = read_varint(bz, i)?;
        i = ni;
        let field = (tag >> 3) as u32;
        match (tag & 7) as u8 {
            0 => {
                let (_, ni) = read_varint(bz, i)?;
                i = ni;
            }
            1 => i += 8,
            5 => i += 4,
            2 => {
                let (len, ni) = read_varint(bz, i)?;
                i = ni;
                let len = len as usize;
                if i + len > bz.len() {
                    return None;
                }
                if field == want {
                    return Some(&bz[i..i + len]);
                }
                i += len;
            }
            _ => return None,
        }
        if i > bz.len() {
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Wrapper encoders.
// ---------------------------------------------------------------------------

/// `ibc.core.client.v1.Height` { revision_number=1, revision_height=2 }.
fn encode_height(h: &Height) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint_field(&mut out, 1, h.revision_number);
    put_varint_field(&mut out, 2, h.revision_height);
    out
}

/// `ibc.lightclients.wasm.v1.ClientState` { data=1, checksum=2, latest_height=3 }.
fn encode_wasm_client_state(data: &[u8], checksum: &[u8], latest: &Height) -> Vec<u8> {
    let mut out = Vec::new();
    put_len(&mut out, 1, data);
    put_len(&mut out, 2, checksum);
    put_len(&mut out, 3, &encode_height(latest));
    out
}

/// `ibc.lightclients.wasm.v1.ConsensusState` { data=1 }.
fn encode_wasm_consensus_state(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_len(&mut out, 1, data);
    out
}

/// `google.protobuf.Any` { type_url=1, value=2 }.
fn encode_any(type_url: &str, value: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_len(&mut out, 1, type_url.as_bytes());
    put_len(&mut out, 2, value);
    out
}

// ---------------------------------------------------------------------------
// Public storage API — the contract's JSON state inside the wasm `data` field.
// ---------------------------------------------------------------------------

/// Persist the client state at the host `clientState` key, wrapped in the
/// `Any`-encoded wasm `ClientState` proto. `checksum` is the wasm code hash the
/// host expects to remain constant.
pub fn save_client_state(
    storage: &mut dyn Storage,
    cs: &ClientState,
    checksum: &[u8],
) -> StdResult<()> {
    let data = to_json_vec(cs)?;
    let value = encode_wasm_client_state(&data, checksum, &cs.latest_height);
    storage.set(CLIENT_STATE_KEY, &encode_any(CLIENT_STATE_TYPE_URL, &value));
    Ok(())
}

/// Load the contract's client state plus the stored checksum (needed to
/// re-encode the wrapper on update). Returns `(state, checksum)`.
pub fn load_client_state(storage: &dyn Storage) -> StdResult<(ClientState, Vec<u8>)> {
    let any = storage
        .get(CLIENT_STATE_KEY)
        .ok_or_else(|| StdError::not_found("clientState"))?;
    let value = get_len_field(&any, 2)
        .ok_or_else(|| StdError::generic_err("clientState Any: missing value"))?;
    let data = get_len_field(value, 1)
        .ok_or_else(|| StdError::generic_err("wasm ClientState: missing data"))?;
    let checksum = get_len_field(value, 2)
        .ok_or_else(|| StdError::generic_err("wasm ClientState: missing checksum"))?
        .to_vec();
    let cs: ClientState = from_json(data)?;
    Ok((cs, checksum))
}

/// Persist a consensus state at `consensusStates/{rev}-{height}`, wrapped in
/// the `Any`-encoded wasm `ConsensusState` proto.
pub fn save_consensus_state(
    storage: &mut dyn Storage,
    h: &Height,
    cs: &ConsensusState,
) -> StdResult<()> {
    let data = to_json_vec(cs)?;
    let value = encode_wasm_consensus_state(&data);
    storage.set(
        &consensus_state_key(h),
        &encode_any(CONSENSUS_STATE_TYPE_URL, &value),
    );
    Ok(())
}

/// Load a consensus state at `consensusStates/{rev}-{height}` (None if absent).
pub fn may_load_consensus_state(
    storage: &dyn Storage,
    h: &Height,
) -> StdResult<Option<ConsensusState>> {
    match storage.get(&consensus_state_key(h)) {
        Some(any) => {
            let value = get_len_field(&any, 2)
                .ok_or_else(|| StdError::generic_err("consensusState Any: missing value"))?;
            let data = get_len_field(value, 1)
                .ok_or_else(|| StdError::generic_err("wasm ConsensusState: missing data"))?;
            Ok(Some(from_json(data)?))
        }
        None => Ok(None),
    }
}
