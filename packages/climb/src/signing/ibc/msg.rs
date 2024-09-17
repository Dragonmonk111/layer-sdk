use std::sync::LazyLock;

use crate::{
    msg_into_google_any,
    querier::{
        abci::AbciProofKind,
        ibc::{IbcChannelProofs, IbcConnectionProofs},
        QueryClient,
    },
    signing::SigningClient,
    IbcChannelId, IbcChannelOrdering, IbcChannelVersion, IbcClientId, IbcConnectionId, IbcPacket,
    IbcPacketTimeoutHeight, IbcPortId,
};
use anyhow::{bail, Context, Result};

// hermes connection handshake: https://github.com/informalsystems/hermes/blob/ccd1d907df4853203349057bba200077254bb83d/crates/relayer/src/connection.rs#L566
// ibc-go connection handshake:
impl SigningClient {
    pub async fn ibc_create_client_msg(
        &self,
        trusting_period_secs: Option<u64>,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::client::v1::MsgCreateClient> {
        let (client_state, consensus_state) = remote_querier
            .ibc_create_client_consensus_state(trusting_period_secs)
            .await?;

        Ok(ibc_proto::ibc::core::client::v1::MsgCreateClient {
            signer: self.addr.to_string(),
            consensus_state: Some(msg_into_google_any(&consensus_state)?),
            client_state: Some(msg_into_google_any(&client_state)?),
        })
    }

    pub async fn ibc_update_client_msg(
        &self,
        client_id: &IbcClientId,
        remote_querier: &QueryClient,
        trusted_height: Option<ibc_proto::ibc::core::client::v1::Height>,
    ) -> Result<ibc_proto::ibc::core::client::v1::MsgUpdateClient> {
        // From Go relayer:
        // > MsgUpdateClient queries for the current client state on dst,
        // > then queries for the latest and trusted headers on src
        // > in order to build a MsgUpdateClient message for dst.

        let trusted_height = match trusted_height {
            None => *self
                .querier
                .ibc_client_state(client_id, None)
                .await?
                .latest_height
                .as_ref()
                .context("missing latest height")?,
            Some(trusted_height) => trusted_height,
        };

        remote_querier
            .wait_until_block_height(trusted_height.revision_height + 1, None)
            .await?;

        // like "srcHeader" in Go relayer
        let curr_signed_header = remote_querier.fetch_signed_header(None).await?;
        let curr_header = curr_signed_header
            .header
            .as_ref()
            .context("missing curr header")?;
        // like "dstTrustedHeader" in Go relayer
        let trusted_signed_header = remote_querier
            .fetch_signed_header(Some(trusted_height.revision_height + 1))
            .await?;
        let trusted_header = trusted_signed_header
            .header
            .as_ref()
            .context("missing trusted header")?;

        let validator_set = remote_querier
            .validator_set(
                Some(curr_header.height.try_into()?),
                Some(&curr_header.proposer_address),
            )
            .await?;
        let trusted_validators = remote_querier
            .validator_set(
                Some(trusted_header.height.try_into()?),
                Some(&trusted_header.proposer_address),
            )
            .await?;

        let header = ibc_proto::ibc::lightclients::tendermint::v1::Header {
            signed_header: Some(curr_signed_header),
            trusted_height: Some(trusted_height),
            validator_set: Some(validator_set),
            trusted_validators: Some(trusted_validators),
        };

        Ok(ibc_proto::ibc::core::client::v1::MsgUpdateClient {
            client_id: client_id.to_string(),
            signer: self.addr.to_string(),
            // this is the ibc header
            // https://github.com/cosmos/relayer/blob/4ed2615217cea7b5e328d3dc2a032bbd8a30df98/relayer/client.go#L372
            // -> https://github.com/cosmos/relayer/blob/4ed2615217cea7b5e328d3dc2a032bbd8a30df98/relayer/chains/cosmos/tx.go#L762
            client_message: Some(msg_into_google_any(&header)?),
        })
    }

    pub async fn ibc_open_connection_init_msg(
        &self,
        client_id: &IbcClientId,
        counterparty_client_id: &IbcClientId,
    ) -> Result<ibc_proto::ibc::core::connection::v1::MsgConnectionOpenInit> {
        Ok(
            ibc_proto::ibc::core::connection::v1::MsgConnectionOpenInit {
                client_id: client_id.to_string(),
                counterparty: Some(ibc_proto::ibc::core::connection::v1::Counterparty {
                    client_id: counterparty_client_id.to_string(),
                    // Go implementation sets this to empty here: https://github.com/cosmos/ibc-go/blob/bb34919be78550e1a2b2da8ad727889ba6a1fc83/modules/core/03-connection/types/msgs.go#L37
                    connection_id: "".to_string(),
                    prefix: Some(IBC_MERKLE_PREFIX.clone()),
                }),
                version: Some(IBC_VERSION.clone()),
                // just used for "time delayed connections": https://ibc.cosmos.network/v8/ibc/overview/#time-delayed-connections
                delay_period: 0,
                signer: self.addr.to_string(),
            },
        )
    }

    pub async fn ibc_open_connection_try_msg(
        &self,
        client_id: &IbcClientId,
        counterparty_client_id: &IbcClientId,
        counterparty_connection_id: &IbcConnectionId,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::connection::v1::MsgConnectionOpenTry> {
        let IbcConnectionProofs {
            proof_height,
            consensus_height,
            connection,
            connection_proof,
            client_state_proof,
            consensus_proof,
            client_state,
            ..
        } = remote_querier
            .ibc_connection_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_client_id,
                counterparty_connection_id,
            )
            .await?;

        if connection.state() != ibc_proto::ibc::core::connection::v1::State::Init {
            bail!(
                "counterparty connection state is not Init, instead it is {:?}",
                connection.state()
            );
        }

        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::connection::v1::MsgConnectionOpenTry {
            client_id: client_id.to_string(),
            client_state: Some(msg_into_google_any(&client_state)?),
            proof_height: Some(proof_height),
            proof_client: client_state_proof,
            proof_init: connection_proof,
            proof_consensus: consensus_proof,
            consensus_height: Some(consensus_height),
            counterparty_versions: vec![IBC_VERSION.clone()],
            counterparty: Some(ibc_proto::ibc::core::connection::v1::Counterparty {
                client_id: counterparty_client_id.to_string(),
                connection_id: counterparty_connection_id.to_string(),
                prefix: Some(IBC_MERKLE_PREFIX.clone()),
            }),
            signer: self.addr.to_string(),
            // hermes doesn't set this field... is it queryable? doesn't seem to be required...
            host_consensus_state_proof: Vec::new(),
            // deprecated
            previous_connection_id: "".to_string(),
            // just used for "time delayed connections": https://ibc.cosmos.network/v8/ibc/overview/#time-delayed-connections
            delay_period: 0,
        })
    }

    pub async fn ibc_open_connection_ack_msg(
        &self,
        client_id: &IbcClientId,
        counterparty_client_id: &IbcClientId,
        connection_id: &IbcConnectionId,
        counterparty_connection_id: &IbcConnectionId,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::connection::v1::MsgConnectionOpenAck> {
        let IbcConnectionProofs {
            query_height,
            proof_height,
            consensus_height,
            connection,
            connection_proof,
            client_state_proof,
            consensus_proof,
            client_state,
        } = remote_querier
            .ibc_connection_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_client_id,
                counterparty_connection_id,
            )
            .await?;

        if connection.state() != ibc_proto::ibc::core::connection::v1::State::Tryopen {
            bail!(
                "counterparty connection state is not TryOpen at height {}, instead it is {:?}",
                query_height,
                connection.state(),
            );
        }

        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::connection::v1::MsgConnectionOpenAck {
            connection_id: connection_id.to_string(),
            counterparty_connection_id: counterparty_connection_id.to_string(),
            client_state: Some(msg_into_google_any(&client_state)?),
            proof_height: Some(proof_height),
            proof_client: client_state_proof,
            proof_try: connection_proof,
            proof_consensus: consensus_proof,
            consensus_height: Some(consensus_height),
            signer: self.addr.to_string(),
            version: Some(IBC_VERSION.clone()),
            // hermes doesn't set this field... is it queryable? doesn't seem to be required...
            host_consensus_state_proof: Vec::new(),
        })
    }

    pub async fn ibc_open_connection_confirm_msg(
        &self,
        client_id: &IbcClientId,
        counterparty_client_id: &IbcClientId,
        connection_id: &IbcConnectionId,
        counterparty_connection_id: &IbcConnectionId,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::connection::v1::MsgConnectionOpenConfirm> {
        let IbcConnectionProofs {
            proof_height,
            connection_proof,
            ..
        } = remote_querier
            .ibc_connection_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_client_id,
                counterparty_connection_id,
            )
            .await?;

        #[allow(deprecated)]
        Ok(
            ibc_proto::ibc::core::connection::v1::MsgConnectionOpenConfirm {
                connection_id: connection_id.to_string(),
                proof_ack: connection_proof,
                proof_height: Some(proof_height),
                signer: self.addr.to_string(),
            },
        )
    }

    pub fn ibc_open_channel_init_msg(
        &self,
        connection_id: &IbcConnectionId,
        port_id: &IbcPortId,
        version: &IbcChannelVersion,
        ordering: IbcChannelOrdering,
        counterparty_port_id: &IbcPortId,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgChannelOpenInit> {
        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::channel::v1::MsgChannelOpenInit {
            port_id: port_id.to_string(),
            channel: Some(ibc_proto::ibc::core::channel::v1::Channel {
                state: ibc_proto::ibc::core::channel::v1::State::Init as i32,
                ordering: match ordering {
                    IbcChannelOrdering::Ordered => {
                        ibc_proto::ibc::core::channel::v1::Order::Ordered as i32
                    }
                    IbcChannelOrdering::Unordered => {
                        ibc_proto::ibc::core::channel::v1::Order::Unordered as i32
                    }
                },
                counterparty: Some(ibc_proto::ibc::core::channel::v1::Counterparty {
                    port_id: counterparty_port_id.to_string(),
                    channel_id: "".to_string(),
                }),
                connection_hops: vec![connection_id.to_string()],
                version: version.to_string(),
                upgrade_sequence: 0,
            }),
            signer: self.addr.to_string(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn ibc_open_channel_try_msg(
        &self,
        client_id: &IbcClientId,
        connection_id: &IbcConnectionId,
        port_id: &IbcPortId,
        version: &IbcChannelVersion,
        counterparty_port_id: &IbcPortId,
        counterparty_channel_id: &IbcChannelId,
        counterparty_version: &IbcChannelVersion,
        ordering: IbcChannelOrdering,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgChannelOpenTry> {
        let IbcChannelProofs {
            proof_height,
            channel_proof,
            ..
        } = remote_querier
            .ibc_channel_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_channel_id,
                counterparty_port_id,
            )
            .await?;

        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::channel::v1::MsgChannelOpenTry {
            port_id: port_id.to_string(),
            previous_channel_id: "".to_string(),
            channel: Some(ibc_proto::ibc::core::channel::v1::Channel {
                state: ibc_proto::ibc::core::channel::v1::State::Tryopen as i32,
                ordering: match ordering {
                    IbcChannelOrdering::Ordered => {
                        ibc_proto::ibc::core::channel::v1::Order::Ordered as i32
                    }
                    IbcChannelOrdering::Unordered => {
                        ibc_proto::ibc::core::channel::v1::Order::Unordered as i32
                    }
                },
                counterparty: Some(ibc_proto::ibc::core::channel::v1::Counterparty {
                    port_id: counterparty_port_id.to_string(),
                    channel_id: counterparty_channel_id.to_string(),
                }),
                connection_hops: vec![connection_id.to_string()],
                version: version.to_string(),
                upgrade_sequence: 0,
            }),
            counterparty_version: counterparty_version.to_string(),
            proof_init: channel_proof,
            proof_height: Some(proof_height),
            signer: self.addr.to_string(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn ibc_open_channel_ack_msg(
        &self,
        client_id: &IbcClientId,
        channel_id: &IbcChannelId,
        port_id: &IbcPortId,
        counterparty_port_id: &IbcPortId,
        counterparty_channel_id: &IbcChannelId,
        counterparty_version: &IbcChannelVersion,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgChannelOpenAck> {
        let IbcChannelProofs {
            proof_height,
            channel_proof,
            ..
        } = remote_querier
            .ibc_channel_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_channel_id,
                counterparty_port_id,
            )
            .await?;

        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::channel::v1::MsgChannelOpenAck {
            port_id: port_id.to_string(),
            channel_id: channel_id.to_string(),
            counterparty_channel_id: counterparty_channel_id.to_string(),
            counterparty_version: counterparty_version.to_string(),
            proof_try: channel_proof,
            proof_height: Some(proof_height),
            signer: self.addr.to_string(),
        })
    }

    pub async fn ibc_open_channel_confirm_msg(
        &self,
        client_id: &IbcClientId,
        channel_id: &IbcChannelId,
        port_id: &IbcPortId,
        counterparty_port_id: &IbcPortId,
        counterparty_channel_id: &IbcChannelId,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgChannelOpenConfirm> {
        let IbcChannelProofs {
            proof_height,
            channel_proof,
            ..
        } = remote_querier
            .ibc_channel_proofs(
                self.querier
                    .ibc_client_state(client_id, None)
                    .await?
                    .latest_height
                    .context("missing latest height")?,
                counterparty_channel_id,
                counterparty_port_id,
            )
            .await?;

        #[allow(deprecated)]
        Ok(ibc_proto::ibc::core::channel::v1::MsgChannelOpenConfirm {
            port_id: port_id.to_string(),
            channel_id: channel_id.to_string(),
            proof_ack: channel_proof,
            proof_height: Some(proof_height),
            signer: self.addr.to_string(),
        })
    }

    pub async fn ibc_packet_recv_msg(
        &self,
        client_id: &IbcClientId,
        packet: IbcPacket,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgRecvPacket> {
        let proof_height = self
            .querier
            .ibc_client_state(client_id, None)
            .await?
            .latest_height
            .context("missing latest height")?;

        let query_height = proof_height.revision_height - 1;

        let packet_commitment_store = remote_querier
            .abci_proof(
                AbciProofKind::IbcPacketCommitment {
                    port_id: packet.src_port_id.clone(),
                    channel_id: packet.src_channel_id.clone(),
                    sequence: packet.sequence,
                },
                query_height,
            )
            .await?;

        if packet_commitment_store.value.is_empty() {
            bail!("packet commitment value is empty");
        }

        if packet_commitment_store.proof.is_empty() {
            bail!("packet commitment proof is empty");
        }

        Ok(ibc_proto::ibc::core::channel::v1::MsgRecvPacket {
            packet: Some(convert_ibc_packet(&packet)?),
            proof_commitment: packet_commitment_store.proof,
            proof_height: Some(proof_height),
            signer: self.addr.to_string(),
        })
    }

    pub async fn ibc_packet_ack_msg(
        &self,
        client_id: &IbcClientId,
        mut packet: IbcPacket,
        remote_querier: &QueryClient,
    ) -> Result<ibc_proto::ibc::core::channel::v1::MsgAcknowledgement> {
        let proof_height = self
            .querier
            .ibc_client_state(client_id, None)
            .await?
            .latest_height
            .context("missing latest height")?;

        let query_height = proof_height.revision_height - 1;

        let packet_ack_store = remote_querier
            .abci_proof(
                AbciProofKind::IbcPacketAck {
                    port_id: packet.src_port_id.clone(),
                    channel_id: packet.src_channel_id.clone(),
                    sequence: packet.sequence,
                },
                query_height,
            )
            .await?;

        if packet_ack_store.value.is_empty() {
            bail!("packet ack value is empty");
        }

        if packet_ack_store.proof.is_empty() {
            bail!("packet ack proof is empty");
        }

        let acknowledgement = packet.ack.take().context("packet ack is missing")?;

        if acknowledgement.is_empty() {
            bail!("acknowledgement is empty");
        }

        // the packet has the correct src->dest in terms of trajectory
        // but it does not reflect the original message, so we need to (re)invert it
        packet.invert();

        Ok(ibc_proto::ibc::core::channel::v1::MsgAcknowledgement {
            packet: Some(convert_ibc_packet(&packet)?),
            acknowledgement,
            proof_acked: packet_ack_store.proof,
            proof_height: Some(proof_height),
            signer: self.addr.to_string(),
        })
    }
}

pub static IBC_VERSION: LazyLock<ibc_proto::ibc::core::connection::v1::Version> =
    LazyLock::new(|| {
        ibc_proto::ibc::core::connection::v1::Version {
            // Go implementation: https://github.com/cosmos/ibc-go/blob/d771177acf66890c9c6f6e5df9a37b8031dbef7d/modules/core/03-connection/types/version.go#L18
            identifier: "1".to_string(),
            // Go implementation: https://github.com/cosmos/ibc-go/blob/d771177acf66890c9c6f6e5df9a37b8031dbef7d/modules/core/03-connection/types/version.go#L22
            features: vec!["ORDER_ORDERED".to_string(), "ORDER_UNORDERED".to_string()],
        }
    });

pub static IBC_MERKLE_PREFIX: LazyLock<ibc_proto::ibc::core::commitment::v1::MerklePrefix> =
    LazyLock::new(|| {
        ibc_proto::ibc::core::commitment::v1::MerklePrefix {
            // Go implementation: https://github.com/cosmos/ibc-go/blob/d771177acf66890c9c6f6e5df9a37b8031dbef7d/modules/core/03-connection/keeper/keeper.go#L53
            // -> https://github.com/cosmos/ibc-go/blob/d771177acf66890c9c6f6e5df9a37b8031dbef7d/modules/core/exported/module.go#L5
            // but also in spec: https://github.com/cosmos/ibc/tree/main/spec/core/ics-003-connection-semantics
            // > Chains should expose an endpoint to allow relayers to query the connection prefix. If not specified, a default counterpartyPrefix of "ibc" should be used.
            // and there doesn't seem to be a universal way to query this, so we'll just use the default (hermes does this too)
            key_prefix: "ibc".as_bytes().to_vec(),
        }
    });

fn convert_ibc_packet(packet: &IbcPacket) -> Result<ibc_proto::ibc::core::channel::v1::Packet> {
    Ok(ibc_proto::ibc::core::channel::v1::Packet {
        sequence: packet.sequence,
        source_port: packet.src_port_id.to_string(),
        source_channel: packet.src_channel_id.to_string(),
        destination_port: packet.dst_port_id.to_string(),
        destination_channel: packet.dst_channel_id.to_string(),
        timeout_height: match packet.timeout_height {
            IbcPacketTimeoutHeight::Revision { revision, height } => {
                Some(ibc_proto::ibc::core::client::v1::Height {
                    revision_number: revision,
                    revision_height: height,
                })
            }
            IbcPacketTimeoutHeight::None => None,
        },
        timeout_timestamp: packet.timeout_timestamp,
        data: packet.data.clone().unwrap_or_default(),
    })
}
