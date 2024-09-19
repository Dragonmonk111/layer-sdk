#![allow(warnings)]
mod opt;

use anyhow::{anyhow, bail, Context, Result};
use bip39::Mnemonic;
use clap::Parser;
use cosmwasm_std::{Addr, Coin};
use layer_climb::{prelude::{KeySigner, TxSigner}, signing::SigningClient};
use opt::{Args, Command, Opt};
use rand::Rng;
use std::{fs, os::unix::net};
use tracing;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().context("couldn't find dotenv file")?;
    let args = Args::parse();

    tracing_subscriber::fmt()
        .without_time()
        .with_target(false)
        .with_max_level(tracing::Level::from(args.log_level))
        .init();

    let opt = Opt::new(args).await?;

    match opt.command {
        Command::WalletShow {} => {
            let signing_client = opt.signing_client().await?;
            tracing::info!("address: {}", signing_client.addr);
            let balances = signing_client
                .querier
                .all_balances(signing_client.addr, None)
                .await?;
            if balances.is_empty() {
                tracing::info!("No balance found");
            } else {
                tracing::info!("Balances:");
                for balance in balances {
                    tracing::info!("{}: {}", balance.denom, balance.amount);
                }
            }
        }
        Command::TapFaucet { amount } => {
            let faucet = opt.faucet_client().await?;
            let addr = opt.address()?;
            let amount = amount.unwrap_or(1_000_000);

            tracing::info!(
                "Balance before: {}",
                faucet
                    .querier
                    .balance(addr.clone(), None)
                    .await?
                    .unwrap_or_default()
            );
            tracing::info!("Sending {} to {}", amount, addr);
            let mut tx_builder = faucet.tx_builder();
            tx_builder.set_gas_simulate_multiplier(2.0);
            faucet
                .transfer(None, amount, addr.clone(), Some(tx_builder))
                .await?;
            tracing::info!(
                "Balance after: {}",
                faucet
                    .querier
                    .balance(addr, None)
                    .await?
                    .unwrap_or_default()
            );
        },

        Command::GenerateWallet {} => {
            let mut rng = rand::thread_rng();
            let entropy: [u8; 32] = rng.gen();
            let mnemonic = Mnemonic::from_entropy(&entropy)?;

            let signer = KeySigner::new_mnemonic_iter(mnemonic.word_iter(), None)?;
            let addr = opt.chain_config.address_from_pub_key(&signer.public_key())?;

            tracing::info!("--- Address ---");
            tracing::info!("{}", addr);
            tracing::info!("--- Mnemonic---");
            tracing::info!("{}", mnemonic);

        }
    }

    Ok(())
}
