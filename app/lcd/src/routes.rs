use rocket::serde::json::Json;

use crate::types::{
    AnnualProvisionsResponse, AuthAccountResponse, BalancesResponse, DelegationResponse,
    DistroParamsResponse, GrantsResponse, InflationResponse, PoolResponse, RewardsResponse,
    SupplyResponse, UnbondingResponse,
};

#[get("/cosmos/auth/v1beta1/accounts/<addr>")]
pub fn auth_account(addr: &str) -> Json<AuthAccountResponse> {
    let sequence = 0u64;
    Json(AuthAccountResponse::new(addr, sequence))
}

#[get("/cosmos/bank/v1beta1/balances/<addr>")]
pub fn balances(addr: &str) -> Json<BalancesResponse> {
    let _ = addr; // explicitly ignore
    let balances = BalancesResponse::new(12345678);
    Json(balances)
}

#[get("/cosmos/authz/v1beta1/grants/granter/<addr>")]
pub fn grants(addr: &str) -> Json<GrantsResponse> {
    let _ = addr; // explicitly ignore
    Json(GrantsResponse::default())
}

#[get("/cosmos/staking/v1beta1/delegations/<addr>")]
pub fn delegations(addr: &str) -> Json<DelegationResponse> {
    let _ = addr; // explicitly ignore
    Json(DelegationResponse::default())
}

#[get("/cosmos/staking/v1beta1/delegators/<addr>/unbonding_delegations")]
pub fn unbonding(addr: &str) -> Json<UnbondingResponse> {
    let _ = addr; // explicitly ignore
    Json(UnbondingResponse::default())
}

#[get("/cosmos/distribution/v1beta1/delegators/<addr>/rewards")]
pub fn rewards(addr: &str) -> Json<RewardsResponse> {
    let _ = addr; // explicitly ignore
    Json(RewardsResponse::default())
}

#[get("/cosmos/mint/v1beta1/annual_provisions")]
pub fn annual_provisions() -> Json<AnnualProvisionsResponse> {
    Json(AnnualProvisionsResponse::default())
}

#[get("/cosmos/staking/v1beta1/pool")]
pub fn staking_pool() -> Json<PoolResponse> {
    Json(PoolResponse::default())
}

#[get("/cosmos/distribution/v1beta1/params")]
pub fn distro_params() -> Json<DistroParamsResponse> {
    Json(DistroParamsResponse::default())
}

#[get("/cosmos/mint/v1beta1/inflation")]
pub fn inflation() -> Json<InflationResponse> {
    Json(InflationResponse::default())
}

#[get("/cosmos/bank/v1beta1/supply/<denom>")]
pub fn supply(denom: &str) -> Json<SupplyResponse> {
    Json(SupplyResponse::new(denom))
}
