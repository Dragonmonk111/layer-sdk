#![allow(warnings)]
mod opt;

use anyhow::{anyhow, bail, Context, Result};
use bip39::Mnemonic;
use clap::Parser;
use cosmwasm_std::{Addr, Coin};
use layer_climb::prelude::*;
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

    match opt.command.clone() {
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
        }

        Command::GenerateWallet {} => {
            let mut rng = rand::thread_rng();
            let entropy: [u8; 32] = rng.gen();
            let mnemonic = Mnemonic::from_entropy(&entropy)?;

            let signer = KeySigner::new_mnemonic_iter(mnemonic.word_iter(), None)?;
            let addr = opt
                .chain_config
                .address_from_pub_key(&signer.public_key())?;

            tracing::info!("--- Address ---");
            tracing::info!("{}", addr);
            tracing::info!("--- Mnemonic---");
            tracing::info!("{}", mnemonic);
        },

        Command::UploadContract { wasm_file } => {
            let wasm_byte_code = tokio::fs::read(wasm_file).await?;
            let client = opt.signing_client().await?;
            let (code_id, tx_resp) = client.contract_upload_file(wasm_byte_code, None).await?;

            tracing::info!("Tx Hash: {}", tx_resp.txhash);
            tracing::info!("Code ID: {}", code_id);
        },

        Command::InstantiateContract {
            code_id,
            msg,
            label,
            funds_denom,
            funds_amount,
        } => {
            let client = opt.signing_client().await?;

            let msg = msg
                .map(ContractMessage::new_raw_str)
                .unwrap_or(ContractMessage::Empty);

            let (addr, tx_resp) = client
                .contract_instantiate(
                    InstantiateParams::new(code_id, label.unwrap_or_default(), msg).set_admin(client.addr.clone()),
                    None,
                )
                .await?; 

            tracing::info!("Tx Hash: {}", tx_resp.txhash);
            tracing::info!("Contract Address: {}", addr);
        },

        Command::ExecuteContract {
            address,
            msg,
            funds_denom,
            funds_amount,
        } => {
            let client = opt.signing_client().await?;

            let msg = msg
                .map(ContractMessage::new_raw_str)
                .unwrap_or(ContractMessage::Empty);

            let address = opt.chain_config.parse_address(&address)?;

            let mut params = ExecuteParams::new(address, msg);

            if let Some(funds_amount) = funds_amount {
                let funds_denom = funds_denom.unwrap_or(opt.chain_config.gas_denom.clone());
                params = params.set_funds(vec![new_coin(funds_denom, funds_amount)]);
            }

            let tx_resp = client.contract_execute(params, None).await?;

            tracing::info!("Tx Hash: {}", tx_resp.txhash);
        },

        Command::QueryContract {
            address,
            msg,
        } => {
            let client = opt.signing_client().await?;

            let msg = msg
                .map(ContractMessage::new_raw_str)
                .unwrap_or(ContractMessage::Empty);

            let address = opt.chain_config.parse_address(&address)?;

            let query = client.querier.contract_smart::<serde_json::Value>(&address, msg).await?.to_string();

            tracing::info!("Query Response: {:?}", query);
        },
    }

    Ok(())
}

