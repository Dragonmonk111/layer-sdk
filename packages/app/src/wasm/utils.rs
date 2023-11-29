use sha2::{
    digest::{Digest, Update},
    Sha256,
};
use thiserror::Error;

use pulsar_std::AccountId;

use crate::PulsarError;

use super::WasmError;

pub fn build_instantiate_address(
    sender: &[u8],
    code_id: u64,
    counter: u64,
) -> Result<AccountId, PulsarError> {
    let mut prehash = Vec::with_capacity(sender.len() + 8 + 8);
    prehash.extend_from_slice(sender);
    prehash.extend(code_id.to_be_bytes());
    prehash.extend(counter.to_be_bytes());
    let raw = Sha256::digest(prehash);
    Ok(AccountId::new(&raw)?)
}

/// We should match the reference wasmd/Go implementation for compatibility with cosmos-sdk:
///     https://github.com/CosmWasm/wasmd/blob/v0.50.0/x/wasm/keeper/addresses.go#L43-L72
///
/// This algorithm can be found in Rust in the cosmwasm-std crate, which we base on:
///     https://github.com/CosmWasm/cosmwasm/blob/v1.5.0/packages/std/src/addresses.rs#L310-L390
///
/// Note, that we make a few changes for the type system, but keep the logic the same
pub fn build_instantiate_2_address(
    checksum: &[u8],
    creator: &AccountId,
    salt: &[u8],
    msg: &[u8],
) -> Result<AccountId, PulsarError> {
    if checksum.len() != 32 {
        return Err(Instantiate2AddressError::InvalidChecksumLength.into());
    }

    if salt.is_empty() || salt.len() > 64 {
        return Err(Instantiate2AddressError::InvalidSaltLength.into());
    };

    let mut key = Vec::<u8>::new();
    key.extend_from_slice(b"wasm\0");
    key.extend_from_slice(&(checksum.len() as u64).to_be_bytes());
    key.extend_from_slice(checksum);
    key.extend_from_slice(&(creator.len() as u64).to_be_bytes());
    key.extend_from_slice(creator);
    key.extend_from_slice(&(salt.len() as u64).to_be_bytes());
    key.extend_from_slice(salt);
    key.extend_from_slice(&(msg.len() as u64).to_be_bytes());
    key.extend_from_slice(msg);
    let address_data = hash("module", &key);
    Ok(AccountId::new(&address_data)?)
}

/// This must be compatible with the wasmd calls, and thus map to address.Module in Cosmos SDK.
///
/// The spec for "Basic Address" Hash can be found here:
/// https://github.com/cosmos/cosmos-sdk/blob/v0.45.8/docs/architecture/adr-028-public-key-addresses.md#module-account-addresses
///
/// The Cosmos SDK implementation of the hash is here:
/// func Module: https://github.com/cosmos/cosmos-sdk/blob/v0.50.0/types/address/hash.go#L66-L86
/// func Hash: https://github.com/cosmos/cosmos-sdk/blob/v0.50.0/types/address/hash.go#L24-L40
fn hash(ty: &str, key: &[u8]) -> Vec<u8> {
    let inner = Sha256::digest(ty.as_bytes());
    Sha256::new().chain(inner).chain(key).finalize().to_vec()
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum Instantiate2AddressError {
    /// Checksum must be 32 bytes
    #[error("invalid checksum length")]
    InvalidChecksumLength,
    /// Salt must be between 1 and 64 bytes
    #[error("invalid salt length")]
    InvalidSaltLength,
}

impl From<Instantiate2AddressError> for PulsarError {
    fn from(value: Instantiate2AddressError) -> Self {
        PulsarError::Wasm(WasmError::Instantiate2Error(value))
    }
}
