use anyhow::{anyhow, Result};
use bip39::Mnemonic;
use cosmrs::{bip32::DerivationPath, crypto::secp256k1::SigningKey};
use std::{str::FromStr, sync::LazyLock};

// https://github.com/confio/cosmos-hd-key-derivation-spec?tab=readme-ov-file#the-cosmos-hub-path
static COSMOS_HUB_PATH: LazyLock<DerivationPath> =
    LazyLock::new(|| DerivationPath::from_str("m/44'/118'/0'/0/0").unwrap());

pub fn cosmos_signing_key<I, S>(mnemonic: I) -> Result<SigningKey> 
where 
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut joined_str = String::new();
    for word in mnemonic {
        joined_str.push_str(word.as_ref());
        joined_str.push(' ');
    } 
    let mnemonic: Mnemonic = joined_str.parse()?;
    SigningKey::derive_from_path(mnemonic.to_seed(""), &COSMOS_HUB_PATH)
        .map_err(|err| anyhow!("{}", err))
}
