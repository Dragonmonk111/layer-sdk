use clap::Parser;
use serde::Serialize;

#[derive(Debug, Parser, Serialize)]
pub struct Cli {
    // Ignored: This is parsed in a first step, but we include here so the parser doesn't error on unknown flag
    /// Provide a home directory for the config files and data directory. Defaults to $HOME/.pulsar
    #[arg(long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub home: Option<String>,

    /// URL of the data layer server.
    #[arg(long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub host: Option<String>,

    #[arg(short, long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub port: Option<u16>,

    /// Log level. One of debug, info, warn, or error
    #[arg(long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub log: Option<String>,

    /// Set to writable directory to use for lmdb storage, otherwise use in-memory storage
    #[arg(long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub lmdb: Option<String>,

    #[arg(long)]
    #[serde(skip_serializing_if = "::std::option::Option::is_none")]
    pub jaeger: Option<String>,
}
