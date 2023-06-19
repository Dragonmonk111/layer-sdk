mod contract;
mod msg;

pub use contract::{execute, instantiate, query, reply};

#[cfg(feature = "library")]
pub use msg::*;
