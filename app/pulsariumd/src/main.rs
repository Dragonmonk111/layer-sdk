use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use tendermint_abci::ServerBuilder;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::FmtSubscriber;

mod app;
mod cli;
mod config;

use crate::app::Pulsarium;
use crate::cli::Cli;
use crate::config::RawConfig;

fn main() {
    // Parse all config info
    let args = Cli::parse();
    // Thanks to https://steezeburger.com/2023/03/rust-hierarchical-configuration/ for this tip
    let config: RawConfig = Figment::from(Serialized::defaults(RawConfig::default()))
        .merge(Toml::file("config/pulsarium.toml"))
        .merge(Env::prefixed("PULSE_"))
        .merge(Serialized::defaults(args))
        .extract()
        .unwrap();
    println!("{:?}", config);
    let config = config.validate().unwrap();

    // set up tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(config.log)
        .with_timer(LocalTime::rfc_3339())
        .with_ansi(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Create the app
    let app = Pulsarium::default();

    // Start ABCI server
    let server = ServerBuilder::new(config.read_buf_size as usize)
        .bind(format!("{}:{}", config.host, config.port), app)
        .unwrap();
    server.listen().unwrap();
}
