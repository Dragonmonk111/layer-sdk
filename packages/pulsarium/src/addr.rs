use std::ops::Deref;

pub const ENV_BECH32_PREFIX: Option<&'static str> = std::option_env!("PULSAR_BECH32");
pub const DEFAULT_BECH32_PREFIX: &str = "pulsar";

fn bech32_prefix() -> &'static str {
    ENV_BECH32_PREFIX.unwrap_or(DEFAULT_BECH32_PREFIX)
}

pub struct Addr(Vec<u8>);

impl Deref for Addr {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

// TODO: to and from bech32

// TODO: from pubkey
