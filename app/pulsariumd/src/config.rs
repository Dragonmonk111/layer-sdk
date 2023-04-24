use serde::{Deserialize, Serialize};

/// The global configuration for Pulsarium
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// The server we listen on (generally 127.0.0.1 or 0.0.0.0)
    pub host: String,

    /// The port we listen on
    pub port: u16,

    /// The log level we use
    pub log: String,
    // /// The directory we read all files from (default $HOME/.pulsarium)
    // pub basedir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "localhost".to_string(),
            port: 26658,
            log: "info".to_string(),
        }
    }
}
