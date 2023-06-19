mod contract;
mod msg;

pub use contract::{execute, instantiate, query};

#[cfg(feature = "library")]
pub use msg::*;
