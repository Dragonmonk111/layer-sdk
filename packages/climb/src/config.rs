use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

use crate::{AddrKind, Address};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChainConfig {
    pub bech32_prefix: String,
    pub chain_id: ChainId,
    pub rpc_endpoint: String,
    pub grpc_endpoint: String,
    pub gas_amount: String,
    pub gas_denom: String,
    pub address_kind: AddrKind,
}

impl ChainConfig {
    pub fn ibc_client_revision(&self) -> Result<u64> {
        // > Tendermint chains wishing to use revisions to maintain persistent IBC connections even across height-resetting upgrades
        // > must format their chainIDs in the following manner: {chainID}-{revision_number}
        // - https://github.com/cosmos/ibc-go/blob/main/docs/docs/01-ibc/01-overview.md#ibc-client-heights
        Ok(self
            .chain_id
            .as_str()
            .split("-")
            .last()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or_default())
    }

    pub fn parse_address(&self, value: impl Into<String>) -> Result<Address> {
        Address::new(&value.into(), self.address_kind.clone())
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ChainId(String);
impl ChainId {
    pub fn new(id: impl ToString) -> Self {
        Self(id.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ChainId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

impl Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
