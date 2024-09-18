// these do not go through request middleware since they are transaction-related
// but they are part of QueryClient, not SigningClient

use std::time::Duration;

use super::QueryClient;
use anyhow::{anyhow, Context, Result};

impl QueryClient {
    pub async fn simulate_tx(
        &self,
        tx_bytes: Vec<u8>,
    ) -> Result<cosmrs::proto::cosmos::tx::v1beta1::SimulateResponse> {
        let mut query_client =
            cosmrs::proto::cosmos::tx::v1beta1::service_client::ServiceClient::new(
                self.grpc_channel.clone(),
            );

        Ok(query_client
            .simulate(
                #[allow(deprecated)]
                cosmrs::proto::cosmos::tx::v1beta1::SimulateRequest { tx: None, tx_bytes },
            )
            .await
            .map(|res| res.into_inner())?)
    }
    pub async fn broadcast_tx_bytes(
        &self,
        tx_bytes: Vec<u8>,
        mode: cosmrs::proto::cosmos::tx::v1beta1::BroadcastMode,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        let mut query_client =
            cosmrs::proto::cosmos::tx::v1beta1::service_client::ServiceClient::new(
                self.grpc_channel.clone(),
            );

        query_client
            .broadcast_tx(cosmrs::proto::cosmos::tx::v1beta1::BroadcastTxRequest {
                tx_bytes,
                mode: mode.into(),
            })
            .await
            .map(|res| res.into_inner().tx_response)?
            .context("couldn't broadcast tx")
    }

    #[tracing::instrument(skip(self))]
    pub async fn poll_until_tx_ready(
        &self,
        tx_hash: String,
        sleep_duration: Duration,
        timeout_duration: Duration,
    ) -> Result<PollTxResponse> {
        let mut query_client =
            cosmrs::proto::cosmos::tx::v1beta1::service_client::ServiceClient::new(
                self.grpc_channel.clone(),
            );

        let mut total_duration = Duration::default();

        loop {
            let response = query_client
                .get_tx(cosmrs::proto::cosmos::tx::v1beta1::GetTxRequest {
                    hash: tx_hash.clone(),
                })
                .await
                .map(|res| {
                    let inner = res.into_inner();
                    (inner.tx, inner.tx_response)
                });

            match response {
                Ok((tx, Some(tx_response))) => {
                    return Ok(PollTxResponse { tx, tx_response });
                }
                Err(e) => {
                    tracing::info!(
                        "failed GetTxRequest [code: {}]. Full error: {:?}",
                        e.code(),
                        e
                    );
                    if e.code() != tonic::Code::Ok && e.code() != tonic::Code::NotFound {
                        return Err(e.into());
                    }
                }
                _ => {}
            }

            futures_timer::Delay::new(sleep_duration).await;
            total_duration += sleep_duration;
            if total_duration >= timeout_duration {
                return Err(anyhow!("timeout"));
            }
        }
    }
}

pub struct PollTxResponse {
    pub tx: Option<cosmrs::proto::cosmos::tx::v1beta1::Tx>,
    pub tx_response: cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse,
}
