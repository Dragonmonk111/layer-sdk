use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};

mod cli;
mod config;

use crate::cli::Cli;
use crate::config::Config;

fn main() {
    // Parse all config info
    let args = Cli::parse();
    // Thanks to https://steezeburger.com/2023/03/rust-hierarchical-configuration/ for this tip
    let config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("config/pulsarium.toml"))
        .merge(Env::prefixed("PULSE_"))
        .merge(Serialized::defaults(args))
        .extract()
        .unwrap();

    println!("{:?}", config);

    // TODO: start server
}
