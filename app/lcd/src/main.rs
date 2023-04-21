#[macro_use]
extern crate rocket;

mod routes;
mod types;

use crate::routes::{
    annual_provisions, auth_account, balances, delegations, distro_params, grants, inflation,
    rewards, staking_pool, supply, unbonding,
};

#[get("/")]
fn index() -> &'static str {
    "Rust LCD Daemon"
}

#[launch]
fn rocket() -> _ {
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
            supply
        ],
    )
}
