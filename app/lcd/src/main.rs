#[macro_use]
extern crate rocket;

use clap::Parser;

mod routes;
mod types;

use crate::routes::{
    annual_provisions, auth_account, balances, broadcast, delegations, distro_params, grants,
    inflation, rewards, simulate, staking_pool, supply, transfer_params, unbonding,
};

#[get("/")]
fn index() -> &'static str {
    "Rust LCD Daemon"
}

#[derive(Parser, Debug)]
#[command(version)]
struct Arguments {
    #[clap(short, long, default_value = "http://localhost:26657")]
    rpc_server: String,
}

#[launch]
fn rocket() -> _ {
    let args = Arguments::parse();
    println!("RPC Server: {}", args.rpc_server);

    rocket::build().mount(
        "/",
        routes![
            index,
            auth_account,
            balances,
            grants,
            delegations,
            unbonding,
            rewards,
            annual_provisions,
            staking_pool,
            distro_params,
            inflation,
            supply,
            simulate,
            transfer_params,
            broadcast,
        ],
    )
}
