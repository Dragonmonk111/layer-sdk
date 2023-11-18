mod auth;
mod bank;
mod cosmwasm;
mod log;
// mod tendermint;
// mod tx;

pub use auth::auth_service;
pub use bank::bank_service;
pub use cosmwasm::cosmwasm_service;
pub use log::LogLayer;
// pub use self::tendermint::{tendermint_service, TendermintService};
// pub use tx::{tx_service, TxService};

use tendermint_proto::abci::RequestQuery;
use tendermint_proto::v0_38::abci::ResponseQuery;

fn grpc_request_to_abci<M: prost::Message>(path: &str, value: &M) -> RequestQuery {
    RequestQuery {
        data: value.encode_to_vec().into(),
        path: path.to_string(),
        height: 0,
        prove: false,
    }
}

fn abci_response_to_grpc<M: prost::Message + Default>(
    response: ResponseQuery,
) -> Result<M, tonic::Status> {
    if response.code != 0 {
        return Err(tonic::Status::new(
            tonic::Code::Internal,
            format!("ABCI error: {}", response.log),
        ));
    }
    M::decode(response.value).map_err(|e| tonic::Status::new(tonic::Code::Internal, e.to_string()))
}
