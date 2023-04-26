use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;
use tracing::metadata::ParseLevelError;

/// Raw user input for the global Pulsarium config
#[derive(Serialize, Deserialize, Debug)]
pub struct RawConfig {
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

impl Default for RawConfig {
    fn default() -> Self {
        RawConfig {
            host: "127.0.0.1".to_string(),
            port: 26658,
            log: "info".to_string(),
            read_buf_size: 4 * 1024 * 1024,
        }
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Using a system port below 1024")]
    ReservedPort,

    #[error("Unknown log level: {0}")]
    InvalidLogLevel(#[from] ParseLevelError),
}

impl RawConfig {
    /// Validate the config and return a type-safe version if okay
    pub fn validate(self) -> Result<Config, ConfigError> {
        if self.port < 1024 {
            return Err(ConfigError::ReservedPort);
        }
        let log = tracing::Level::from_str(&self.log)?;
        // TODO: validate host
        Ok(Config {
            host: self.host,
            port: self.port,
            log,
            read_buf_size: self.read_buf_size,
        })
    }
}

/// The global configuration for Pulsarium
#[derive(Debug)]
pub struct Config {
    /// The server we listen on (generally 127.0.0.1 or 0.0.0.0)
    pub host: String,

    /// The port we listen on
    pub port: u16,

    /// The log level we use
    pub log: tracing::Level,

    pub read_buf_size: u32,
    // /// The directory we read all files from (default $HOME/.pulsarium)
    // pub basedir: String,
}
