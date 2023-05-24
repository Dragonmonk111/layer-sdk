use std::fmt::{Display, Formatter, self};

pub struct HexEncode<'a>(&'a [u8]);

impl<'a> HexEncode<'a> {
    pub fn new(bytes: &'a impl AsRef<[u8]>) -> HexEncode<'a> {
        HexEncode(bytes.as_ref())
    }
}

impl Display for HexEncode<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}