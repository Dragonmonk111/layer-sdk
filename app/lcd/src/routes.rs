use cosmwasm_std::Coin;

use rocket::data::ToByteUnit;
use rocket::serde::json::Json;
use rocket::{tokio, Data};

use tendermint_rpc::{Client, HttpClient, Error as TmError};

use crate::types::{
    AnnualProvisionsResponse, AuthAccountResponse, BalancesResponse, BroadcastResponse,
    DelegationResponse, DistroParamsResponse, GrantsResponse, InflationResponse, PoolResponse,
    RewardsResponse, SimulateResponse, SupplyResponse, TransferResponse, UnbondingResponse,
};

use crate::types::BaseCoin;

pub struct State {
    pub client: HttpClient,
}

// use cosmrs::proto::cosmos::bank::v1beta1::{QueryAllBalancesRequest, QueryAllBalancesResponse};


// TODO: this is horrible hack for basecoin-rs not implementing sdk...
// later use the proper grpc types and path
pub async fn query_balances(client: &HttpClient, addr: &str) -> Result<Vec<Coin>, TmError> {
    // let query = QueryAllBalancesRequest{ address: addr.to_string(), pagination: None };
    let query = addr.as_bytes();
    let res = client.abci_query(Some("/custom/bank/???".to_string()), query, None, false).await?;
    if res.code.is_err() {
        return Err(TmError::server(format!("Query Error Code: {:?}", res.code)));
    }
    let res: Vec<BaseCoin> = serde_json::from_slice(&res.value).unwrap();
    Ok(res.into_iter().map(Into::into).collect())
}

#[get("/cosmos/auth/v1beta1/accounts/<addr>")]
pub fn auth_account(addr: &str) -> Json<AuthAccountResponse> {
    let sequence = 0u64;
    Json(AuthAccountResponse::new(addr, sequence))
}

#[get("/cosmos/bank/v1beta1/balances/<addr>")]
pub fn balances(addr: &str) -> Json<BalancesResponse> {
    let _ = addr; // explicitly ignore
    let balances = BalancesResponse::new(15750000);
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

#[get("/ibc/apps/transfer/v1/params")]
pub fn transfer_params() -> Json<TransferResponse> {
    Json(TransferResponse::default())
}

// TODO: use the data
#[post("/cosmos/tx/v1beta1/simulate")]
pub fn simulate() -> Json<SimulateResponse> {
    Json(SimulateResponse::default())
}

// TODO: use the data
#[post("/cosmos/tx/v1beta1/txs", data = "<data>")]
pub async fn broadcast(data: Data<'_>) -> std::io::Result<Json<BroadcastResponse>> {
    data.open(512.kibibytes())
        .stream_to(tokio::io::stdout())
        .await?;
    let res = BroadcastResponse::default();
    println!("{:?}", res);
    Ok(Json(res))
}
