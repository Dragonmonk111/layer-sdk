use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use pulsar_abci::ServerConfig;
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

#[tokio::main]
async fn main() {
    // Parse all config info
    let args = Cli::parse();
    // Thanks to https://steezeburger.com/2023/03/rust-hierarchical-configuration/ for this tip
    let config: RawConfig = Figment::from(Serialized::defaults(RawConfig::default()))
        .merge(Toml::file("config/pulsarium.toml"))
        .merge(Env::prefixed("PULSE_"))
        .merge(Serialized::defaults(args))
        .extract()
        .unwrap();
    // We print this out for debugging before the logger is set up
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
            .with_service_name("pulsariumd")
            .install_batch(opentelemetry::runtime::Tokio)
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
    match config.lmdb {
        Some(path) => {
            info!("using lmdb database at {}", path);
            let storage = pulsar_storage::LmdbStore::new(&path, None);
            let app = Pulsarium::new(storage);

            // Start ABCI server
            let server = ServerConfig::new()
                .with_read_buf(config.read_buf_size as usize)
                .bind(format!("{}:{}", config.host, config.port), app)
                .await
                .unwrap();
            server.listen().await.unwrap();
        }
        None => {
            info!("using in-memory database");
            let storage = pulsar_storage::MemoryStore::new();
            let app = Pulsarium::new(storage);

            // Start ABCI server
            let server = ServerConfig::new()
                .with_read_buf(config.read_buf_size as usize)
                .bind(format!("{}:{}", config.host, config.port), app)
                .await
                .unwrap();
            server.listen().await.unwrap();
        }
    }

    // proper shutdown
    if config.jaeger {
        opentelemetry::global::shutdown_tracer_provider();
    }
}
