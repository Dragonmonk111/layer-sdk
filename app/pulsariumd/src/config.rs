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

    pub read_buf_size: u32,
    // /// The directory we read all files from (default $HOME/.pulsarium)
    // pub basedir: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "127.0.0.1".to_string(),
            port: 26658,
            log: "info".to_string(),
            read_buf_size: 4 * 1024 * 1024,
        }
    }
}

impl Config {
    // TODO: define error type
    /// Return an error if this is invalid
    pub fn validate(&self) -> Result<(), String> {
        // TODO: implement
        if self.port < 1024 {
            return Err(format!("bad port: {}", self.port));
        }
        Ok(())
    }
}
