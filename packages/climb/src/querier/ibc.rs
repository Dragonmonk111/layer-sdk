use anyhow::{bail, Context, Result};
use cosmrs::proto::prost::Message;

use crate::{apply_grpc_height, IbcChannelId, IbcClientId, IbcConnectionId, IbcPortId};

use super::{
    abci::{AbciProofKind, AbciProofReq},
    basic::{BlockHeaderReq, BlockHeightReq, StakingParamsReq},
    QueryClient, QueryRequest,
};

impl QueryClient {
    pub async fn ibc_connection_proofs(
        &self,
        proof_height: ibc_proto::ibc::core::client::v1::Height,
        client_id: &IbcClientId,
        connection_id: &IbcConnectionId,
    ) -> Result<IbcConnectionProofs> {
        self.run_with_middleware(IbcConnectionProofsReq {
            proof_height,
            client_id: client_id.clone(),
            connection_id: connection_id.clone(),
        })
        .await
    }

    pub async fn ibc_channel_proofs(
        &self,
        proof_height: ibc_proto::ibc::core::client::v1::Height,
        channel_id: &IbcChannelId,
        port_id: &IbcPortId,
    ) -> Result<IbcChannelProofs> {
        self.run_with_middleware(IbcChannelProofsReq {
            proof_height,
            channel_id: channel_id.clone(),
            port_id: port_id.clone(),
        })
        .await
    }

    pub async fn ibc_client_state(
        &self,
        ibc_client_id: &IbcClientId,
        height: Option<u64>,
    ) -> Result<ibc_proto::ibc::lightclients::tendermint::v1::ClientState> {
        self.run_with_middleware(IbcClientStateReq {
            ibc_client_id: ibc_client_id.clone(),
            height,
        })
        .await
    }

    pub async fn ibc_connection(
        &self,
        connection_id: &IbcConnectionId,
        height: Option<u64>,
    ) -> Result<ibc_proto::ibc::core::connection::v1::ConnectionEnd> {
        self.run_with_middleware(IbcConnectionReq {
            connection_id: connection_id.clone(),
            height,
        })
        .await
    }

    pub async fn ibc_connection_consensus_state(
        &self,
        connection_id: &IbcConnectionId,
        consensus_height: Option<ibc_proto::ibc::core::client::v1::Height>,
        height: Option<u64>,
    ) -> Result<tendermint_proto::google::protobuf::Any> {
        self.run_with_middleware(IbcConnectionConsensusStateReq {
            connection_id: connection_id.clone(),
            consensus_height,
            height,
        })
        .await
    }

    pub async fn ibc_channel(
        &self,
        channel_id: &IbcChannelId,
        port_id: &IbcPortId,
        height: Option<u64>,
    ) -> Result<ibc_proto::ibc::core::channel::v1::Channel> {
        self.run_with_middleware(IbcChannelReq {
            channel_id: channel_id.clone(),
            port_id: port_id.clone(),
            height,
        })
        .await
    }

    pub async fn ibc_create_client_consensus_state(
        &self,
        trusting_period_secs: Option<u64>,
    ) -> Result<(
        ibc_proto::ibc::lightclients::tendermint::v1::ClientState,
        ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState,
    )> {
        self.run_with_middleware(IbcCreateClientConsensusStateReq {
            trusting_period_secs,
        })
        .await
    }
}

#[derive(Clone, Debug)]
struct IbcConnectionProofsReq {
    pub proof_height: ibc_proto::ibc::core::client::v1::Height,
    pub client_id: IbcClientId,
    pub connection_id: IbcConnectionId,
}

impl QueryRequest for IbcConnectionProofsReq {
    type QueryResponse = IbcConnectionProofs;

    async fn request(&self, client: QueryClient) -> Result<IbcConnectionProofs> {
        let IbcConnectionProofsReq {
            proof_height,
            client_id,
            connection_id,
        } = self;

        let query_height = proof_height.revision_height - 1;

        let connection = IbcConnectionReq {
            connection_id: connection_id.clone(),
            height: Some(query_height),
        }
        .request(client.clone())
        .await?;
        let connection_proof = AbciProofReq {
            kind: AbciProofKind::IbcConnection {
                connection_id: connection_id.clone(),
            },
            height: query_height,
        }
        .request(client.clone())
        .await?
        .proof;

        let client_state = IbcClientStateReq {
            ibc_client_id: client_id.clone(),
            height: Some(query_height),
        }
        .request(client.clone())
        .await?;
        let client_state_proof = AbciProofReq {
            kind: AbciProofKind::IbcClientState {
                client_id: client_id.clone(),
            },
            height: query_height,
        }
        .request(client.clone())
        .await?
        .proof;

        let consensus_height = *client_state
            .latest_height
            .as_ref()
            .context("missing client state latest height")?;

        let consensus_proof = AbciProofReq {
            kind: AbciProofKind::IbcConsensus {
                client_id: client_id.clone(),
                height: consensus_height,
            },
            height: query_height,
        }
        .request(client.clone())
        .await?
        .proof;

        if client_state_proof.is_empty() {
            bail!("missing client state proof");
        }
        if connection_proof.is_empty() {
            bail!("missing connection proof");
        }
        if consensus_proof.is_empty() {
            bail!("missing consensus proof");
        }

        Ok(IbcConnectionProofs {
            proof_height: *proof_height,
            consensus_height,
            query_height,
            connection,
            connection_proof,
            client_state_proof,
            consensus_proof,
            client_state,
        })
    }
}

#[derive(Clone, Debug)]
struct IbcChannelProofsReq {
    pub proof_height: ibc_proto::ibc::core::client::v1::Height,
    pub channel_id: IbcChannelId,
    pub port_id: IbcPortId,
}

impl QueryRequest for IbcChannelProofsReq {
    type QueryResponse = IbcChannelProofs;

    async fn request(&self, client: QueryClient) -> Result<IbcChannelProofs> {
        let IbcChannelProofsReq {
            proof_height,
            channel_id,
            port_id,
        } = self;

        let query_height = proof_height.revision_height - 1;

        let channel = IbcChannelReq {
            channel_id: channel_id.clone(),
            port_id: port_id.clone(),
            height: Some(query_height),
        }
        .request(client.clone())
        .await?;
        let channel_proof = AbciProofReq {
            kind: AbciProofKind::IbcChannel {
                channel_id: channel_id.clone(),
                port_id: port_id.clone(),
            },
            height: query_height,
        }
        .request(client)
        .await?
        .proof;

        Ok(IbcChannelProofs {
            proof_height: *proof_height,
            query_height,
            channel,
            channel_proof,
        })
    }
}

#[derive(Clone, Debug)]
struct IbcClientStateReq {
    pub ibc_client_id: IbcClientId,
    pub height: Option<u64>,
}

impl QueryRequest for IbcClientStateReq {
    type QueryResponse = ibc_proto::ibc::lightclients::tendermint::v1::ClientState;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<ibc_proto::ibc::lightclients::tendermint::v1::ClientState> {
        let IbcClientStateReq {
            ibc_client_id,
            height,
        } = self;

        let mut req =
            tonic::Request::new(ibc_proto::ibc::core::client::v1::QueryClientStateRequest {
                client_id: ibc_client_id.to_string(),
            });

        apply_grpc_height(&mut req, *height)?;

        let mut query_client = ibc_proto::ibc::core::client::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );
        let resp: ibc_proto::ibc::core::client::v1::QueryClientStateResponse = query_client
            .client_state(req)
            .await
            .map(|res| res.into_inner())
            .context("couldn't get client state")?;

        let client_state = resp
            .client_state
            .map(|client_state| match client_state.type_url.as_str() {
                "/ibc.lightclients.tendermint.v1.ClientState" => {
                    ibc_proto::ibc::lightclients::tendermint::v1::ClientState::decode(
                        client_state.value.as_slice(),
                    )
                    .map_err(|e| e.into())
                }
                _ => Err(anyhow::anyhow!(
                    "unsupported client state type: {}",
                    client_state.type_url
                )),
            })
            .transpose()?
            .context("missing client state")?;

        Ok(client_state)
    }
}

#[derive(Clone, Debug)]
struct IbcConnectionReq {
    pub connection_id: IbcConnectionId,
    pub height: Option<u64>,
}

impl QueryRequest for IbcConnectionReq {
    type QueryResponse = ibc_proto::ibc::core::connection::v1::ConnectionEnd;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<ibc_proto::ibc::core::connection::v1::ConnectionEnd> {
        let IbcConnectionReq {
            connection_id,
            height,
        } = self;

        let mut req = tonic::Request::new(
            ibc_proto::ibc::core::connection::v1::QueryConnectionRequest {
                connection_id: connection_id.to_string(),
            },
        );

        apply_grpc_height(&mut req, *height)?;

        let mut query_client = ibc_proto::ibc::core::connection::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        query_client
            .connection(req)
            .await
            .map(|res| res.into_inner())
            .context("couldn't get connection")?
            .connection
            .context("missing connection")
    }
}

#[derive(Clone, Debug)]
struct IbcConnectionConsensusStateReq {
    pub connection_id: IbcConnectionId,
    pub consensus_height: Option<ibc_proto::ibc::core::client::v1::Height>,
    pub height: Option<u64>,
}

impl QueryRequest for IbcConnectionConsensusStateReq {
    type QueryResponse = tendermint_proto::google::protobuf::Any;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<tendermint_proto::google::protobuf::Any> {
        let IbcConnectionConsensusStateReq {
            connection_id,
            consensus_height,
            height,
        } = self;

        let mut query_client = ibc_proto::ibc::core::connection::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let consensus_height = match consensus_height {
            Some(h) => *h,
            None => ibc_proto::ibc::core::client::v1::Height {
                revision_number: client.chain_config.ibc_client_revision()?,
                revision_height: match height {
                    Some(h) => *h,
                    None => BlockHeightReq {}.request(client).await?,
                },
            },
        };

        let mut req = tonic::Request::new(
            ibc_proto::ibc::core::connection::v1::QueryConnectionConsensusStateRequest {
                connection_id: connection_id.to_string(),
                revision_number: consensus_height.revision_number,
                revision_height: consensus_height.revision_height,
            },
        );

        apply_grpc_height(&mut req, *height)?;

        query_client
            .connection_consensus_state(req)
            .await
            .map(|res| res.into_inner())
            .context("couldn't get consensus state")?
            .consensus_state
            .context("missing consensus state")
    }
}

#[derive(Clone, Debug)]
struct IbcChannelReq {
    pub channel_id: IbcChannelId,
    pub port_id: IbcPortId,
    pub height: Option<u64>,
}

impl QueryRequest for IbcChannelReq {
    type QueryResponse = ibc_proto::ibc::core::channel::v1::Channel;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::Channel> {
        let IbcChannelReq {
            channel_id,
            port_id,
            height,
        } = self;

        let mut req = tonic::Request::new(ibc_proto::ibc::core::channel::v1::QueryChannelRequest {
            channel_id: channel_id.to_string(),
            port_id: port_id.to_string(),
        });

        apply_grpc_height(&mut req, *height)?;

        let mut query_client = ibc_proto::ibc::core::channel::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        query_client
            .channel(req)
            .await
            .map(|res| res.into_inner())
            .context("couldn't get channel")?
            .channel
            .context("missing channel")
    }
}

#[derive(Clone, Debug)]
struct IbcCreateClientConsensusStateReq {
    pub trusting_period_secs: Option<u64>,
}

impl QueryRequest for IbcCreateClientConsensusStateReq {
    type QueryResponse = (
        ibc_proto::ibc::lightclients::tendermint::v1::ClientState,
        ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState,
    );

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<(
        ibc_proto::ibc::lightclients::tendermint::v1::ClientState,
        ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState,
    )> {
        let trusting_period_secs = self.trusting_period_secs;

        let latest_block_header = BlockHeaderReq { height: None }
            .request(client.clone())
            .await?;

        let consensus_state = ibc_proto::ibc::lightclients::tendermint::v1::ConsensusState {
            timestamp: latest_block_header.time(),
            root: Some(ibc_proto::ibc::core::commitment::v1::MerkleRoot {
                // in MerkleRoot comment itself: "In the Cosmos SDK, the AppHash of a block header becomes the root."
                hash: latest_block_header.app_hash(),
            }),
            next_validators_hash: latest_block_header.next_validators_hash(),
        };

        let staking_params = StakingParamsReq {}.request(client.clone()).await?;

        let unbonding_period = staking_params
            .unbonding_time
            .context("missing unbonding time")?;

        let unbonding_period = tendermint_proto::google::protobuf::Duration {
            seconds: unbonding_period.seconds,
            nanos: unbonding_period.nanos,
        };

        // 2/3 of the unbonding period gives enough time to trust without constant checking
        // but still within enough time to punish misbehaviour
        let trusting_period = match trusting_period_secs {
            Some(trusting_period_secs) => tendermint_proto::google::protobuf::Duration {
                seconds: trusting_period_secs.try_into()?,
                nanos: 0,
            },
            None => tendermint_proto::google::protobuf::Duration {
                seconds: (unbonding_period.seconds * 2) / 3,
                nanos: (unbonding_period.nanos * 2) / 3,
            },
        };

        // value taken from ibc-go tests: https://github.com/cosmos/ibc-go/blob/049bef96f730ee7f29647b1d5833530444395abc/testing/values.go#L33
        let max_clock_drift = tendermint_proto::google::protobuf::Duration {
            seconds: 10,
            nanos: 0,
        };

        let chain_id = client.chain_config.chain_id.to_string();

        let latest_height = ibc_proto::ibc::core::client::v1::Height {
            revision_number: client.chain_config.ibc_client_revision()?,
            revision_height: latest_block_header.height()?,
        };

        #[allow(deprecated)]
        let client_state = ibc_proto::ibc::lightclients::tendermint::v1::ClientState {
            chain_id,
            // https://github.com/cosmos/ibc-go/blob/049bef96f730ee7f29647b1d5833530444395abc/modules/light-clients/07-tendermint/fraction.go#L9
            // -> https://github.com/cometbft/cometbft/blob/27a460641ad835b9e6ae47523c12b0678b4619a8/light/verifier.go#L15
            trust_level: Some(ibc_proto::ibc::lightclients::tendermint::v1::Fraction {
                numerator: 1,
                denominator: 3,
            }),
            trusting_period: Some(trusting_period),
            unbonding_period: Some(unbonding_period),
            max_clock_drift: Some(max_clock_drift),
            frozen_height: None,
            latest_height: Some(latest_height),
            // https://github.com/cosmos/ibc-go/blob/0613ec84a1a38ca797931343f7e2da330ec7c508/modules/core/23-commitment/types/merkle.go#L18
            proof_specs: vec![
                ibc_proto::ics23::iavl_spec(),
                ibc_proto::ics23::tendermint_spec(),
            ],
            // in the ClientState definition itself:
            // > For SDK chains using the default upgrade module, upgrade_path should be []string{"upgrade", "upgradedIBCState"}`
            upgrade_path: vec!["upgrade".to_string(), "upgradedIBCState".to_string()],
            allow_update_after_expiry: false,
            allow_update_after_misbehaviour: false,
        };

        Ok((client_state, consensus_state))
    }
}

#[derive(Debug, Clone)]
pub struct IbcConnectionProofs {
    pub proof_height: ibc_proto::ibc::core::client::v1::Height,
    pub consensus_height: ibc_proto::ibc::core::client::v1::Height,
    pub query_height: u64,
    pub connection: ibc_proto::ibc::core::connection::v1::ConnectionEnd,
    pub connection_proof: Vec<u8>,
    pub client_state_proof: Vec<u8>,
    pub consensus_proof: Vec<u8>,
    pub client_state: ibc_proto::ibc::lightclients::tendermint::v1::ClientState,
}

#[derive(Debug, Clone)]
pub struct IbcChannelProofs {
    pub proof_height: ibc_proto::ibc::core::client::v1::Height,
    pub query_height: u64,
    pub channel: ibc_proto::ibc::core::channel::v1::Channel,
    pub channel_proof: Vec<u8>,
}
