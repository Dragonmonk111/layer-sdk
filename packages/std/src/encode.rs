use std::fmt::{self, Display, Formatter};

pub struct HexEncode<'a>(&'a [u8]);

impl<'a> HexEncode<'a> {
    pub fn new(bytes: &'a impl AsRef<[u8]>) -> HexEncode<'a> {
        HexEncode(bytes.as_ref())
    }
}

impl Display for HexEncode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode_upper(self.0))
    }
}

pub struct CoinEncode<'a>(pub &'a [cosmwasm_std::Coin]);

impl Display for CoinEncode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for coin in self.0 {
            write!(f, "{}{},", coin.amount, coin.denom)?;
        }
        Ok(())
    }
}
