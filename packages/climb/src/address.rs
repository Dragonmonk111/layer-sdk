use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

/// The canonical type used everywhere for addresses
/// the internal representation is as a String, so that
/// it's cheap to impl Display, which is the common usecase
/// however, by keeping the AddrKind around, we can do conversions
/// to and from different byte-level representations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Address {
    pub value: String,
    pub kind: AddrKind,
}

impl Address {
    // this will attempt to validate the address
    // if you already know that it's valid, just create the struct directly
    pub fn new(value: &str, kind: AddrKind) -> Result<Self> {
        match &kind {
            AddrKind::Cosmos { prefix } => {
                let account_id: cosmrs::AccountId = value.parse().map_err(|e| anyhow!("{e:?}"))?;
                if account_id.prefix() != prefix {
                    bail!("Address prefix does not match expected prefix");
                }

                Ok(Self {
                    value: value.to_string(),
                    kind,
                })
            }
            AddrKind::Eth => {
                AddrEth::try_from(value)?;
                Ok(Self {
                    value: value.to_string(),
                    kind,
                })
            }
        }
    }

    pub fn new_pub_key(pub_key: &cosmrs::crypto::PublicKey, kind: AddrKind) -> Result<Self> {
        match &kind {
            AddrKind::Cosmos { prefix } => {
                let account_id = pub_key.account_id(prefix).map_err(|e| anyhow!("{e:?}"))?;
                Ok(Self {
                    value: account_id.to_string(),
                    kind,
                })
            }
            AddrKind::Eth => {
                bail!("TODO - implement pub_key to eth addr");
            }
        }
    }

    // for native conversions, all the From/Into traits are implemented
    // but sometimes we want to convert from one kind to another
    pub fn convert_into_cosmos(&self, prefix: String) -> Result<Self> {
        match &self.kind {
            AddrKind::Cosmos { .. } => Ok(Self {
                value: self.value.clone(),
                kind: AddrKind::Cosmos { prefix },
            }),
            AddrKind::Eth => {
                bail!("TODO - implement eth to cosmos addr");
            }
        }
    }

    pub fn convert_into_eth(&self) -> Result<Self> {
        match &self.kind {
            AddrKind::Cosmos { .. } => {
                bail!("TODO - implement cosmos to eth addr");
            }
            AddrKind::Eth => Ok(self.clone()),
        }
    }
}

// the display impl ignores the kind
impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AddrKind {
    Eth,
    Cosmos { prefix: String },
}

///// Cosmos address
impl From<&cosmrs::AccountId> for Address {
    fn from(addr: &cosmrs::AccountId) -> Self {
        Address {
            value: addr.to_string(),
            kind: AddrKind::Cosmos {
                prefix: addr.prefix().to_string(),
            },
        }
    }
}

impl TryFrom<&Address> for cosmrs::AccountId {
    type Error = anyhow::Error;

    fn try_from(addr: &Address) -> Result<Self> {
        match &addr.kind {
            AddrKind::Cosmos { prefix } => {
                cosmrs::AccountId::new(prefix, addr.value.as_bytes()).map_err(|e| anyhow!("{e:?}"))
            }
            AddrKind::Eth => {
                bail!("Address must be Cosmos - use convert_into_cosmos() instead");
            }
        }
    }
}

impl From<cosmrs::AccountId> for Address {
    fn from(addr: cosmrs::AccountId) -> Self {
        (&addr).into()
    }
}

impl TryFrom<Address> for cosmrs::AccountId {
    type Error = anyhow::Error;

    fn try_from(addr: Address) -> Result<Self> {
        (&addr).try_into()
    }
}

///// Ethereum address
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AddrEth([u8; 20]);

impl AddrEth {
    pub fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> [u8; 20] {
        self.0
    }
}

impl From<&AddrEth> for Address {
    fn from(addr: &AddrEth) -> Self {
        Address {
            value: format!("0x{}", hex::encode(addr.0)),
            kind: AddrKind::Eth,
        }
    }
}

impl TryFrom<&Address> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(addr: &Address) -> Result<Self> {
        match addr.kind {
            AddrKind::Eth => addr.to_string().try_into(),
            AddrKind::Cosmos { .. } => {
                bail!("Address must be Ethereum, use convert_into_eth() instead");
            }
        }
    }
}

impl TryFrom<&[u8]> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        Ok(Self::new(bytes.try_into().map_err(|e| anyhow!("{e:?}"))?))
    }
}

impl TryFrom<&str> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self> {
        // strip off leading "0x"
        let s = s.strip_prefix("0x").unwrap_or(s);
        hex::decode(s)?.try_into()
    }
}

impl From<AddrEth> for Address {
    fn from(addr: AddrEth) -> Self {
        (&addr).into()
    }
}

impl TryFrom<Address> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(addr: Address) -> Result<Self> {
        (&addr).try_into()
    }
}

impl TryFrom<Vec<u8>> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(bytes: Vec<u8>) -> Result<Self> {
        bytes.as_slice().try_into()
    }
}

impl TryFrom<String> for AddrEth {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        s.as_str().try_into()
    }
}

#[cfg(test)]
mod test {
    use cosmrs::AccountId;

    use crate::AddrKind;

    use super::{AddrEth, Address};

    // TODO get addresses that are actually the same underlying public key

    const TEST_COSMOS_STR: &str = "osmo1suhgf5svhu4usrurvxzlgn54ksxmn8gljarjtxqnapv8kjnp4nrsll0sqv";
    const TEST_ETH_STR: &str = "0xb794f5ea0ba39494ce839613fffba74279579268";

    #[test]
    fn test_basic_roundtrip_eth() {
        let test_string = TEST_ETH_STR;
        let addr_bytes: AddrEth = test_string.try_into().unwrap();
        let addr_string: Address = (&addr_bytes).into();

        assert_eq!(addr_string.to_string(), test_string);
        assert_eq!(addr_string.kind, AddrKind::Eth);

        let addr_bytes2: AddrEth = addr_string.try_into().unwrap();
        assert_eq!(addr_bytes2, addr_bytes);
    }

    #[test]
    fn test_basic_roundtrip_cosmos() {
        let test_string = TEST_COSMOS_STR;
        let account_id: AccountId = test_string.parse().unwrap();
        let addr_string: Address = (&account_id).try_into().unwrap();

        assert_eq!(addr_string.to_string(), test_string);
        assert!(matches!(addr_string.kind, AddrKind::Cosmos { .. }));

        let account_id_2: AccountId = addr_string.try_into().unwrap();
        assert_eq!(account_id_2, account_id);
    }

    #[test]
    fn test_convert_eth_to_cosmos() {
        // let test_string = "0xb794f5ea0ba39494ce839613fffba74279579268";
        // let addr_bytes:AddrEth = test_string.try_into().unwrap();
        // let addr_string:Address = (&addr_bytes).into();
        // let addr_string_cosmos = addr_string.convert_into_cosmos("osmo".to_string()).unwrap();
        // assert_eq!(addr_string_cosmos.to_string(), "osmo1suhgf5svhu4usrurvxzlgn54ksxmn8gljarjtxqnapv8kjnp4nrsll0sqv");
    }

    #[test]
    fn test_convert_cosmos_to_eth() {
        // let test_string = "osmo1suhgf5svhu4usrurvxzlgn54ksxmn8gljarjtxqnapv8kjnp4nrsll0sqv";
        // let account_id:AccountId = test_string.parse().unwrap();
        // let addr_string:Address = (&account_id).try_into().unwrap();
        // let addr_string_eth = addr_string.convert_into_eth().unwrap();
        // assert_eq!(addr_string_eth.to_string(), "0xb794f5ea0ba39494ce839613fffba74279579268");
    }
}
