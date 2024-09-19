use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cosmwasm_std::Coin;
use serde::{Deserialize, Serialize};
use layer_climb::prelude::*;

// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_0/index.html

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(long, value_enum, default_value_t = TargetEnvironment::Local)]
    pub target_env: TargetEnvironment,

    /// Set the logging level
    #[arg(long, value_enum, default_value_t = LogLevel::Info)]
    //#[arg(long, value_enum, default_value_t = LogLevel::Debug)]
    pub log_level: LogLevel,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Copy, Clone, Debug, clap::ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for tracing::Level {
    fn from(log_level: LogLevel) -> Self {
        match log_level {
            LogLevel::Trace => tracing::Level::TRACE,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TargetEnvironment {
    Local,
    Testnet,
}

#[derive(Subcommand)]
pub enum Command {
    /// Shows the wallet balance and address
    WalletShow {},

    /// Taps the faucet to get some funds
    TapFaucet {
        #[arg(long)]
        amount: Option<u128>,
    },

    /// Generates a random wallet. 
    /// Shows the mnemonic and address.
    GenerateWallet
}

pub struct Opt {
    pub command: Command,
    pub chain_config: ChainConfig,
    mnemonic: String,
    faucet_config: FaucetConfig,
}

impl Opt {
    pub async fn new(args: Args) -> Result<Self> {
        let mnemonic = match args.target_env {
            TargetEnvironment::Local => std::env::var("LOCAL_MNEMONIC"),
            TargetEnvironment::Testnet => std::env::var("TEST_MNEMONIC"),
        }
        .context("Mnemonic not found")?;

        let configs: Config = serde_json::from_str(include_str!("../config.json"))
            .context("Failed to parse config")?;

        let chain_config = match args.target_env {
            TargetEnvironment::Local => configs.chains.local,
            TargetEnvironment::Testnet => configs.chains.testnet,
        }
        .context(format!(
            "Chain config for environment {:?} not found",
            args.target_env
        ))?;

        Ok(Opt {
            command: args.command,
            chain_config,
            mnemonic,
            faucet_config: configs.faucet,
        })
    }

    pub fn signer(&self) -> Result<KeySigner> {
        KeySigner::new_mnemonic_str(&self.mnemonic, None)
    }


    pub fn address(&self) -> Result<Address> {
        self.chain_config
            .address_from_pub_key(&self.signer()?.public_key())
    }

    pub async fn query_client(&self) -> Result<QueryClient> {
        QueryClient::new(self.chain_config.clone()).await
    }

    pub async fn signing_client(&self) -> Result<SigningClient> {
        SigningClient::new(self.chain_config.clone(), self.signer()?).await
    }

    pub async fn faucet_client(&self) -> Result<SigningClient> {
        let signer = KeySigner::new_mnemonic_str(&self.faucet_config.mnemonic, None)?;
        SigningClient::new(self.chain_config.clone(), signer).await
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    pub chains: ChainConfigs,
    pub faucet: FaucetConfig,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChainConfigs {
    pub local: Option<ChainConfig>,
    pub testnet: Option<ChainConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FaucetConfig {
    pub mnemonic: String,
}
