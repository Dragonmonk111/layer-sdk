use std::fmt::{Display, Formatter};
use std::ops::Deref;

use bech32::{self, Error as Bech32Error, FromBase32, ToBase32, Variant};
use cosmwasm_std::StdResult;
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use thiserror::Error;

pub const ENV_BECH32_PREFIX: Option<&'static str> = std::option_env!("PULSAR_BECH32");
pub const DEFAULT_BECH32_PREFIX: &str = "pulsar";

/// Valid lengths of decoded addresses
pub const VALID_ADDR_LENGTH: [usize; 2] = [20usize, 32usize];

fn bech32_prefix() -> &'static str {
    ENV_BECH32_PREFIX.unwrap_or(DEFAULT_BECH32_PREFIX)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Addr(Vec<u8>);

impl Deref for Addr {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

#[derive(Error, Debug)]
pub enum AddrError {
    /// TODO: normalize this, so we don't have possibly non-deterministic errors from different crate versions
    #[error("{0}")]
    Bech32(#[from] Bech32Error),

    #[error("Invalid variant: bech32m")]
    InvalidVariant,

    #[error("Invalid prefix: {0} expected {1}")]
    InvalidPrefix(String, &'static str),

    #[error("Invalid address size: {0} bytes")]
    InvalidLength(usize),
}

impl Display for Addr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        bech32::encode_to_fmt(f, bech32_prefix(), self.0.to_base32(), Variant::Bech32).unwrap()
    }
}

impl Into<String> for &Addr {
    fn into(self) -> String {
        self.to_string()
    }
}

impl Addr {
    pub fn parse_string(encoded: &str) -> Result<Self, AddrError> {
        let (hrp, data, variant) = bech32::decode(encoded)?;
        // no bech32m
        if variant != Variant::Bech32 {
            return Err(AddrError::InvalidVariant);
        }
        // make sure the proper chain prefix
        let prefix = bech32_prefix();
        if hrp != prefix {
            return Err(AddrError::InvalidPrefix(hrp, prefix));
        }
        let addr = Vec::<u8>::from_base32(&data).unwrap();
        // we only support 20 and 32 bytes for the binary version, enforce this for sanity check
        if !VALID_ADDR_LENGTH.contains(&addr.len()) {
            return Err(AddrError::InvalidLength(addr.len()));
        }
        Ok(Addr(addr))
    }

    // only for use in test
    pub fn unchecked(name: &str) -> Self {
        // pad to valid length
        let l = VALID_ADDR_LENGTH[0];
        let mut v = vec![0u8; l];
        v.copy_from_slice(name.as_bytes());
        Addr(v)
    }
}

impl<'a> PrimaryKey<'a> for Addr {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl<'a> Prefixer<'a> for Addr {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl KeyDeserialize for Addr {
    type Output = Addr;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(Addr(value))
    }
}

impl<'a> PrimaryKey<'a> for &'a Addr {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = Self;
    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl<'a> Prefixer<'a> for &'a Addr {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.deref())]
    }
}

impl KeyDeserialize for &Addr {
    type Output = Addr;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        Ok(Addr(value))
    }
}

// TODO: from pubkey
