pub use cosmwasm_std::Timestamp;

use std::ops::Add;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration(u64);

impl Duration {
    pub fn from_seconds(seconds: u64) -> Self {
        Self(seconds * 1_000_000_000)
    }

    pub fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    pub fn from_parts(seconds: u64, nanos: u64) -> Self {
        Self(seconds * 1_000_000_000 + nanos)
    }

    pub fn to_parts(self) -> (u64, u64) {
        (self.0 / 1_000_000_000, self.0 % 1_000_000_000)
    }

    pub fn after(self, time: Timestamp) -> Timestamp {
        time.plus_nanos(self.0)
    }
}

impl Add<Timestamp> for Duration {
    type Output = Timestamp;

    fn add(self, time: Timestamp) -> Timestamp {
        self.after(time)
    }
}
