use std::env;
use std::path::{Path, PathBuf};

use clap::Parser;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use pulsar_abci::ServerConfig;
use pulsar_app::AppConfig;
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

/// We check fro pulsar home dir:
/// * from --home flag
/// * from PULSE_HOME env var
/// * default to $HOME/.pulsar
fn get_home() -> PathBuf {
    let mut pargs = pico_args::Arguments::from_env();

    // check for --home flag
    if let Some(home) = pargs.opt_value_from_str::<_, String>("--home").unwrap() {
        return PathBuf::from(home);
    }

    // check PULSE_HOME
    if let Ok(pulse) = env::var("PULSE_HOME") {
        return PathBuf::from(pulse);
    }

    // default to $HOME/.pulsar
    Path::new(&env::var("HOME").unwrap()).join(".pulsar")
}

#[tokio::main]
async fn main() {
    let home = get_home();
    let config_file = home.as_path().join("config/pulsarium.toml");
    println!("Reading config file from {}", config_file.to_str().unwrap());

    // Parse all config info
    let args = Cli::parse();
    // Thanks to https://steezeburger.com/2023/03/rust-hierarchical-configuration/ for this tip
    let config: RawConfig = Figment::from(Serialized::defaults(RawConfig::default()))
        .merge(Toml::file(config_file))
        .merge(Env::prefixed("PULSE_"))
        .merge(Serialized::defaults(args))
        .extract()
        .unwrap();
    // We print this out for debugging before the logger is set up
    println!("{:?}", config);
    let config = config.validate().unwrap();

    // add open telemetry
    if config.jaeger {
        opentelemetry::global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
        let tracer = opentelemetry_jaeger::new_collector_pipeline()
            .with_endpoint("http://localhost:14268/api/traces")
            //         // optionally set username and password as well.
            //         // .with_username("username")
            //         // .with_password("s3cr3t")
            .with_service_name("pulsariumd")
            .with_isahc()
            .with_timeout(std::time::Duration::from_secs(2))
            .install_batch(opentelemetry::runtime::Tokio)
            .unwrap();
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = tracing_subscriber::Registry::default()
            .with(config.filter)
            .with(telemetry);

        // let subscriber = fmt_subscriber.with(telemetry);
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
        info!("jaeger tracing enabled");
    } else {
        let fmt_subscriber = FmtSubscriber::builder()
            .with_env_filter(config.filter)
            .with_timer(LocalTime::rfc_3339())
            .with_ansi(true)
            .finish();

        tracing::subscriber::set_global_default(fmt_subscriber)
            .expect("setting default subscriber failed");
    }

    // autogenerate wasm_dir from home
    let wasm_path = home.as_path().join("data");
    let wasm_dir = wasm_path.to_str().unwrap();
    let app_config = AppConfig::new(wasm_dir);

    // Create the app
    match config.lmdb {
        Some(path) => {
            info!("using lmdb database at {}", path);
            let storage = pulsar_storage::LmdbStore::new(&path, None);
            let app = Pulsarium::new(storage, app_config);

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
            let app = Pulsarium::new(storage, app_config);

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
