#[macro_use]
extern crate rocket;

use clap::Parser;
use tendermint_rpc::{Client, HttpClient};

mod routes;
mod types;

use crate::routes::{annual_provisions, auth_account, balances, broadcast, delegations, distro_params, grants, inflation, rewards, simulate, staking_pool, supply, transfer_params, unbonding, State, query_balances};

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
async fn rocket() -> _ {
    let args = Arguments::parse();
    println!("RPC Server: {}", args.rpc_server);

    // let client = HttpClient::new("http://127.0.0.1:26657").unwrap();
    let client = HttpClient::new(args.rpc_server.as_str()).unwrap();
    let abci_info = client.abci_info().await.unwrap();
    println!("ABCI Info: {:?}", abci_info);

    let balance = query_balances(&client, "cosmos1eaulhtty6er8e3huz8c4wktz82vf8krnqtl8vr").await.unwrap();
    println!("Balance: {:?}", balance);

    let my_routes = routes![
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
    ];

    rocket::build()
        .manage(State { client })
        .mount("/", my_routes)
}
