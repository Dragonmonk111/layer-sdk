use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use tendermint_abci::ServerBuilder;
use tracing::info;
use tracing_subscriber::fmt::time::LocalTime;
use tracing_subscriber::prelude::*;
use tracing_subscriber::FmtSubscriber;

mod app;
mod cli;
mod config;
mod convert;
mod decode;
mod encode;

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
    let fmt_subscriber = FmtSubscriber::builder()
        .with_env_filter(config.filter)
        .with_timer(LocalTime::rfc_3339())
        .with_ansi(true)
        .finish();

    // add open telemetry
    if config.jaeger {
        opentelemetry::global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
        let tracer = opentelemetry_jaeger::new_agent_pipeline()
            .install_simple()
            .unwrap();
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = fmt_subscriber.with(telemetry);
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
        info!("jaeger tracing enabled");

    //     let tracer = opentelemetry_jaeger::new_collector_pipeline()
    //         .with_endpoint("http://localhost:14268/api/traces")
    //         // optionally set username and password as well.
    //         // .with_username("username")
    //         // .with_password("s3cr3t")
    //         .install_batch().unwrap();
    } else {
        tracing::subscriber::set_global_default(fmt_subscriber)
            .expect("setting default subscriber failed");
    }

    // Create the app
    let app = Pulsarium::default();

    // Start ABCI server
    let server = ServerBuilder::new(config.read_buf_size as usize)
        .bind(format!("{}:{}", config.host, config.port), app)
        .unwrap();
    server.listen().unwrap();

    // proper shutdown
    if config.jaeger {
        opentelemetry::global::shutdown_tracer_provider();
    }
}
