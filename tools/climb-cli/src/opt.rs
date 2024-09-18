use std::{path::PathBuf, str::FromStr};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use cosmwasm_std::Coin;
use layer_climb::{cosmrs::crypto::secp256k1::SigningKey, querier::QueryClient, signing::{key::cosmos_signing_key, SigningClient}, AddrKind, Address, ChainConfig};
use serde::{Deserialize, Serialize};

// https://docs.rs/clap/latest/clap/_derive/_tutorial/chapter_0/index.html

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, value_enum, default_value_t = TargetEnvironment::Local)]
    pub target_env: TargetEnvironment,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TargetEnvironment {
    Local,
    Testnet,
}

#[derive(Subcommand)]
pub enum Command {
    WalletShow {
    },
    TapFaucet {
        #[arg(long)]
        amount: Option<u128>,
    },
}

pub struct Opt {
    pub command: Command,
    pub chain_config: ChainConfig,
    mnemonic: String,
    faucet_config: FaucetConfig,
}

impl Opt {
    pub async fn parse() -> Result<Self> {

        let args = Args::parse();

        let mnemonic = match args.target_env {
            TargetEnvironment::Local => std::env::var("LOCAL_MNEMONIC"),
            TargetEnvironment::Testnet => std::env::var("TEST_MNEMONIC")
        }.context("Mnemonic not found")?;

        let configs: Config = serde_json::from_str(include_str!("../config.json")).context("Failed to parse config")?;

        let chain_config = match args.target_env {
            TargetEnvironment::Local => configs.chains.local,
            TargetEnvironment::Testnet => configs.chains.testnet
        }.context(format!("Chain config for environment {:?} not found", args.target_env))?;

        Ok(Opt {
            command: args.command,
            chain_config,
            mnemonic,
            faucet_config: configs.faucet
        })
    }

    pub fn signing_key(&self) -> Result<SigningKey> {
        cosmos_signing_key(self.mnemonic.split(" "))
    }

    pub fn address(&self) -> Result<Address> {
        let addr = Address::new_pub_key(&self.signing_key()?.public_key(), self.chain_config.address_kind.clone())?;
        Ok(addr)
    }

    pub async fn query_client(&self) -> Result<QueryClient> {
        QueryClient::new(self.chain_config.clone()).await
    }

    pub async fn signing_client(&self) -> Result<SigningClient> {
        SigningClient::new(self.chain_config.clone(), None, self.signing_key()?).await
    }

    pub async fn faucet_client(&self) -> Result<SigningClient> {
        let signing_key = cosmos_signing_key(self.faucet_config.mnemonic.split(" "))?;
        SigningClient::new(self.chain_config.clone(), None, signing_key).await
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    pub chains: ChainConfigs,
    pub faucet: FaucetConfig
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