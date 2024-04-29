use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;

use bech32::{self, Error as Bech32Error, FromBase32, ToBase32, Variant};
use cosmwasm_std::StdResult;
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use thiserror::Error;

pub const ENV_BECH32_PREFIX: Option<&'static str> = std::option_env!("SLAY_BECH32");
// pub const DEFAULT_BECH32_PREFIX: &str = "slay3r";
pub const DEFAULT_BECH32_PREFIX: &str = "pulsar";

/// Valid lengths of decoded addresses
pub const VALID_ADDR_LENGTH: [usize; 2] = [20usize, 32usize];

fn bech32_prefix() -> &'static str {
    ENV_BECH32_PREFIX.unwrap_or(DEFAULT_BECH32_PREFIX)
}

// Note: this is expanded cw_serde macro minus the Debug implementation, as we want to use Display there
#[derive(
    ::cosmwasm_schema::serde::Serialize,
    ::cosmwasm_schema::serde::Deserialize,
    ::std::clone::Clone,
    ::std::cmp::PartialEq,
    ::cosmwasm_schema::schemars::JsonSchema,
)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[serde(deny_unknown_fields, crate = "::cosmwasm_schema::serde")]
#[schemars(crate = "::cosmwasm_schema::schemars")]
#[derive(Hash, Eq)]
pub struct AccountId(Vec<u8>);

impl Deref for AccountId {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum AccountIdError {
    /// FIXME: normalize this, so we don't have possibly non-deterministic errors from different crate versions
    #[error("Bech32: {0}")]
    Bech32(String),

    #[error("Invalid variant: bech32m")]
    InvalidVariant,

    #[error("Invalid prefix: {0} expected {1}")]
    InvalidPrefix(String, &'static str),

    #[error("Invalid address size: {0} bytes")]
    InvalidLength(usize),
}

impl From<Bech32Error> for AccountIdError {
    fn from(value: Bech32Error) -> Self {
        AccountIdError::Bech32(value.to_string())
    }
}

impl Display for AccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        bech32::encode_to_fmt(f, bech32_prefix(), self.0.to_base32(), Variant::Bech32).unwrap()
    }
}

impl Debug for AccountId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl From<&AccountId> for String {
    fn from(value: &AccountId) -> Self {
        value.to_string()
    }
}

/// This is meant as a helper for testcode
/// Panics on error
pub fn must_id(str: &str) -> AccountId {
    AccountId::parse_string(str).unwrap()
}

impl AccountId {
    /// This takes
    pub fn new(raw: &[u8]) -> Result<Self, AccountIdError> {
        if !VALID_ADDR_LENGTH.contains(&raw.len()) {
            Err(AccountIdError::InvalidLength(raw.len()))
        } else {
            Ok(AccountId(raw.to_vec()))
        }
    }

    pub fn parse_string(encoded: &str) -> Result<Self, AccountIdError> {
        let (hrp, data, variant) = bech32::decode(encoded)?;
        // no bech32m
        if variant != Variant::Bech32 {
            return Err(AccountIdError::InvalidVariant);
        }
        // make sure the proper chain prefix
        let prefix = bech32_prefix();
        if hrp != prefix {
            return Err(AccountIdError::InvalidPrefix(hrp, prefix));
        }
        let addr = Vec::<u8>::from_base32(&data).unwrap();
        // we only support 20 and 32 bytes for the binary version, enforce this for sanity check
        if !VALID_ADDR_LENGTH.contains(&addr.len()) {
            return Err(AccountIdError::InvalidLength(addr.len()));
        }
        Ok(AccountId(addr))
    }

    // only for use in test
    pub fn unchecked(name: &str) -> Self {
        // pad to valid length
        let mut v = name.as_bytes().to_vec();
        v.resize(VALID_ADDR_LENGTH[0], 0u8);
        AccountId(v)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl<'a> PrimaryKey<'a> for AccountId {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl<'a> Prefixer<'a> for AccountId {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl KeyDeserialize for AccountId {
    type Output = AccountId;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(AccountId(value))
    }
}

impl KeyDeserialize for &AccountId {
    type Output = AccountId;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(AccountId(value))
    }
}
