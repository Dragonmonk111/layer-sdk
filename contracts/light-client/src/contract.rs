//! 08-wasm light client contract entrypoints.
//!
//! Driven by ibc-go's `08-wasm` module via the standard
//! `instantiate`/`sudo`/`query` wire API (see `msg.rs` for the exact
//! payload shapes, mirrored from ibc-go's `contract_api.go`).

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{from_json, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError};
use sha2::{Digest, Sha256};

use crate::error::ContractError;
use crate::merkle::{self, BlockPayloadMirror};
use crate::msg::{
    CheckForMisbehaviourResponse, Header, InstantiateMsg, MembershipProof, Misbehaviour, QueryMsg,
    StatusResponse, SudoMsg, TimestampAtHeightResponse, UpdateStateResponse,
};
use crate::state::{ClientState, ConsensusState, Height, CLIENT_STATE, CONSENSUS_STATES};
use crate::verify::{verify_certificate, DecodedProposal};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let client_state: ClientState = from_json(msg.client_state.as_slice())?;
    let consensus_state: ConsensusState = from_json(msg.consensus_state.as_slice())?;

    // Validate the group public key eagerly so a malformed ceremony output
    // fails at client creation, not on the first header.
    let pk_bytes =
        hex::decode(&client_state.group_public_key_hex).map_err(|_| ContractError::InvalidPublicKey)?;
    if pk_bytes.len() != 96 {
        return Err(ContractError::InvalidPublicKeyLength(pk_bytes.len()));
    }

    CONSENSUS_STATES.save(
        deps.storage,
        (
            client_state.latest_height.revision_number,
            client_state.latest_height.revision_height,
        ),
        &consensus_state,
    )?;
    CLIENT_STATE.save(deps.storage, &client_state)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("chain_id", &client_state.chain_id))
}

/// Runs §6 verification for a header against the current client state.
/// Does not mutate storage — callers decide whether to persist.
fn check_header(client_state: &ClientState, header: &Header) -> Result<DecodedProposal, ContractError> {
    if let Some(frozen_at) = &client_state.frozen_height {
        return Err(ContractError::ClientFrozen(frozen_at.revision_height));
    }

    // v1: single revision (see spec §11 / state.rs Height docs).
    if header.height.revision_number != client_state.latest_height.revision_number {
        return Err(ContractError::InvalidProposal);
    }

    let pk_bytes = hex::decode(&client_state.group_public_key_hex)
        .map_err(|_| ContractError::InvalidPublicKey)?;
    verify_certificate(
        &pk_bytes,
        header.proposal_bytes.as_slice(),
        header.certificate_bytes.as_slice(),
    )
}

/// §6 + monotonicity + dedup. Returns the decoded proposal on success.
fn check_update_header(
    deps: Deps,
    client_state: &ClientState,
    header: &Header,
) -> Result<DecodedProposal, ContractError> {
    let decoded = check_header(client_state, header)?;

    if header.height.revision_height <= client_state.latest_height.revision_height {
        // Allow the exact same header at the latest height (idempotent
        // replay by a relayer); reject anything strictly older or a
        // conflicting header at the same height.
        if header.height.revision_height == client_state.latest_height.revision_height {
            let existing = CONSENSUS_STATES
                .may_load(
                    deps.storage,
                    (
                        header.height.revision_number,
                        header.height.revision_height,
                    ),
                )?
                .ok_or(ContractError::ConsensusStateNotFound(
                    header.height.revision_height,
                ))?;
            if existing.payload_digest_hex == hex::encode(decoded.payload) {
                return Ok(decoded);
            }
        }
        return Err(ContractError::NonMonotonicHeight {
            header: header.height.revision_height,
            latest: client_state.latest_height.revision_height,
        });
    }

    // Equivocation guard: a conflicting but otherwise valid header at a
    // height we already store is misbehaviour, not an update — the caller
    // must submit it via update_state_on_misbehaviour instead.
    if let Some(existing) = CONSENSUS_STATES.may_load(
        deps.storage,
        (header.height.revision_number, header.height.revision_height),
    )? {
        if existing.payload_digest_hex != hex::encode(decoded.payload) {
            return Err(ContractError::VerificationFailed);
        }
    }

    Ok(decoded)
}

/// §7 misbehaviour: two headers that both verify under §6 but equivocate.
fn check_misbehaviour(
    client_state: &ClientState,
    misbehaviour: &Misbehaviour,
) -> Result<(), ContractError> {
    let decoded_a = check_header(client_state, &misbehaviour.header_a)?;
    let decoded_b = check_header(client_state, &misbehaviour.header_b)?;

    let same_round = decoded_a.epoch == decoded_b.epoch && decoded_a.view == decoded_b.view;
    let same_height =
        misbehaviour.header_a.height.revision_height == misbehaviour.header_b.height.revision_height;
    if !same_round && !same_height {
        return Err(ContractError::MisbehaviourMismatch);
    }
    if decoded_a.payload == decoded_b.payload {
        return Err(ContractError::NoEquivocation);
    }
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, _env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        SudoMsg::UpdateState { client_message } => {
            let mut client_state = CLIENT_STATE.load(deps.storage)?;
            let header: Header = from_json(client_message.as_slice())?;
            let decoded = check_update_header(deps.as_ref(), &client_state, &header)?;

            let consensus_state = ConsensusState {
                payload_digest_hex: hex::encode(decoded.payload),
                timestamp: header.timestamp,
                epoch: decoded.epoch,
                view: decoded.view,
                parent: decoded.parent,
            };
            CONSENSUS_STATES.save(
                deps.storage,
                (header.height.revision_number, header.height.revision_height),
                &consensus_state,
            )?;
            client_state.latest_height = header.height;
            CLIENT_STATE.save(deps.storage, &client_state)?;

            let result = UpdateStateResponse {
                heights: vec![header.height],
            };
            Ok(Response::new().set_data(to_json_binary(&result)?))
        }
        SudoMsg::UpdateStateOnMisbehaviour { client_message } => {
            let mut client_state = CLIENT_STATE.load(deps.storage)?;
            let misbehaviour: Misbehaviour = from_json(client_message.as_slice())?;
            check_misbehaviour(&client_state, &misbehaviour)?;

            let freeze_height = misbehaviour
                .header_a
                .height
                .revision_height
                .min(misbehaviour.header_b.height.revision_height);
            client_state.frozen_height = Some(Height::new(0, freeze_height));
            CLIENT_STATE.save(deps.storage, &client_state)?;

            // EmptyResult
            Ok(Response::new())
        }
        SudoMsg::VerifyMembership {
            height,
            delay_time_period: _,
            delay_block_period: _,
            proof,
            merkle_path,
            value,
        } => {
            // 1. Consensus state at the proof height → expected payload
            //    digest. The proof verifies against the state committed by
            //    THIS height's payload (post-state of height-1 — app-hash
            //    semantics), so relayers pass proof_height = state_height+1.
            let consensus = CONSENSUS_STATES
                .may_load(
                    deps.storage,
                    (height.revision_number, height.revision_height),
                )?
                .ok_or(ContractError::ConsensusStateNotFound(
                    height.revision_height,
                ))?;

            // 2. Parse our proof format (JSON inside the opaque proof bytes).
            let proof: MembershipProof = from_json(proof.as_slice())?;

            // 3. Bind the proof to the signed certificate chain:
            //    sha256(payload_bytes) must equal the stored payload digest.
            let digest = Sha256::digest(proof.payload_bytes.as_slice());
            if hex::encode(digest) != consensus.payload_digest_hex {
                return Err(ContractError::InvalidMembershipProof);
            }

            // 4. Extract state_root from the signed payload — the root is
            //    only trustworthy because it came from inside the bytes the
            //    threshold certificate covers.
            let payload: BlockPayloadMirror =
                bincode::deserialize(proof.payload_bytes.as_slice())
                    .map_err(|_| ContractError::InvalidMembershipProof)?;

            // 5. Leaf key = concatenated key_path elements (the chain stores
            //    IBC commitments under keys equal to their ICS-24 path).
            let key: Vec<u8> = merkle_path
                .key_path
                .iter()
                .flat_map(|p| p.as_slice().iter().copied())
                .collect();
            let leaf = merkle::leaf_hash(&key, value.as_slice());

            // 6. Walk the sibling path to the state root.
            let mut siblings = Vec::with_capacity(proof.siblings.len());
            for s in &proof.siblings {
                siblings.push(match s {
                    Some(b) => Some(
                        <[u8; 32]>::try_from(b.as_slice())
                            .map_err(|_| ContractError::InvalidMembershipProof)?,
                    ),
                    None => None,
                });
            }
            let root = merkle::compute_root(leaf, proof.leaf_index, &siblings);
            if root != payload.state_root {
                return Err(ContractError::InvalidMembershipProof);
            }

            // EmptyResult — ibc-go treats success as membership proven.
            Ok(Response::new())
        }
        SudoMsg::VerifyNonMembership {
            height: _,
            delay_time_period: _,
            delay_block_period: _,
            proof: _,
            merkle_path: _,
        } => Err(ContractError::MembershipProofsUnsupported),
        SudoMsg::VerifyUpgradeAndUpdateState { .. } => Err(ContractError::Unsupported),
        SudoMsg::MigrateClientStore {} => {
            // No store migration needed for this client (v1 layouts are
            // stable); acknowledge with EmptyResult.
            Ok(Response::new())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::Status {} => {
            let client_state = CLIENT_STATE.load(deps.storage)?;
            let status = if client_state.frozen_height.is_some() {
                "Frozen"
            } else {
                "Active"
            };
            to_json_binary(&StatusResponse {
                status: status.to_string(),
            })
        }
        QueryMsg::TimestampAtHeight { height } => {
            let consensus_state = CONSENSUS_STATES
                .load(deps.storage, (height.revision_number, height.revision_height))
                .map_err(|_| StdError::not_found("ConsensusState"))?;
            to_json_binary(&TimestampAtHeightResponse {
                timestamp: consensus_state.timestamp,
            })
        }
        QueryMsg::VerifyClientMessage { client_message } => {
            let client_state = CLIENT_STATE.load(deps.storage)?;
            // Either a header (§6) or a misbehaviour report (§7) is valid.
            let result: Result<(), ContractError> = if let Ok(header) =
                from_json::<Header>(client_message.as_slice())
            {
                check_header(&client_state, &header).map(|_| ())
            } else if let Ok(misbehaviour) =
                from_json::<Misbehaviour>(client_message.as_slice())
            {
                check_misbehaviour(&client_state, &misbehaviour)
            } else {
                Err(ContractError::InvalidProposal)
            };
            match result {
                Ok(()) => Ok(Binary::default()),
                Err(e) => Err(StdError::generic_err(e.to_string())),
            }
        }
        QueryMsg::CheckForMisbehaviour { client_message } => {
            let client_state = CLIENT_STATE.load(deps.storage)?;
            let found = match from_json::<Misbehaviour>(client_message.as_slice()) {
                Ok(misbehaviour) => check_misbehaviour(&client_state, &misbehaviour).is_ok(),
                Err(_) => false,
            };
            to_json_binary(&CheckForMisbehaviourResponse {
                found_misbehaviour: found,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::to_json_vec;
    use crate::msg::MerklePath;

    fn build_client_state(pubkey_hex: &str, latest_height: u64) -> ClientState {
        ClientState {
            chain_id: "junoclaw-devnet-1".to_string(),
            group_public_key_hex: pubkey_hex.to_string(),
            latest_height: Height::from_block_height(latest_height),
            frozen_height: None,
        }
    }

    fn setup(
        pubkey_hex: &str,
        latest_height: u64,
        consensus_state: &ConsensusState,
    ) -> cosmwasm_std::OwnedDeps<
        cosmwasm_std::testing::MockStorage,
        cosmwasm_std::testing::MockApi,
        cosmwasm_std::testing::MockQuerier,
    > {
        let mut deps = mock_dependencies();
        let creator = deps.api.addr_make("creator");
        let msg = InstantiateMsg {
            client_state: Binary::new(to_json_vec(&build_client_state(pubkey_hex, latest_height)).unwrap()),
            consensus_state: Binary::new(to_json_vec(consensus_state).unwrap()),
            checksum: Binary::default(),
        };
        instantiate(
            deps.as_mut(),
            mock_env(),
            MessageInfo {
                funds: vec![],
                sender: creator,
            },
            msg,
        )
        .unwrap();
        deps
    }

    // Reuse the real DKG signing path from verify.rs tests so contract-level
    // tests exercise genuine blst-backed threshold certificates.
    fn make_header(height: u64, timestamp: u64, payload: [u8; 32]) -> (Header, String) {
        let (pubkey, proposal_bytes, cert_bytes) = crate::verify::tests::build_certified_proposal(
            height,
            0,
            height,
            height.saturating_sub(1),
            payload,
        );
        (
            Header {
                height: Height::from_block_height(height),
                timestamp,
                proposal_bytes: Binary::new(proposal_bytes),
                certificate_bytes: Binary::new(cert_bytes),
            },
            hex::encode(pubkey),
        )
    }

    #[test]
    fn instantiate_stores_state() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 1,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header, pubkey_hex) = make_header(10, 1_000_000, [7u8; 32]);
        let _ = header;
        let deps = setup(&pubkey_hex, 10, &cs);

        let loaded = CLIENT_STATE.load(&deps.storage).unwrap();
        assert_eq!(loaded.latest_height.revision_height, 10);
        assert!(loaded.frozen_height.is_none());
        let stored_cs = CONSENSUS_STATES
            .load(&deps.storage, (0, 10))
            .unwrap();
        assert_eq!(stored_cs.timestamp, 1);
    }

    #[test]
    fn update_state_advances_client() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header, pubkey_hex) = make_header(11, 2_000_000_000, [9u8; 32]);
        let mut deps = setup(&pubkey_hex, 10, &cs);

        let sudo_msg = SudoMsg::UpdateState {
            client_message: Binary::new(to_json_vec(&header).unwrap()),
        };
        let res = sudo(deps.as_mut(), mock_env(), sudo_msg).unwrap();
        let parsed: UpdateStateResponse = from_json(res.data.unwrap()).unwrap();
        assert_eq!(parsed.heights, vec![Height::from_block_height(11)]);

        let client = CLIENT_STATE.load(&deps.storage).unwrap();
        assert_eq!(client.latest_height.revision_height, 11);
        let stored_cs = CONSENSUS_STATES.load(&deps.storage, (0, 11)).unwrap();
        assert_eq!(stored_cs.timestamp, 2_000_000_000);
        assert_eq!(stored_cs.payload_digest_hex, hex::encode([9u8; 32]));
    }

    #[test]
    fn update_state_rejects_stale_header() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header, pubkey_hex) = make_header(9, 0, [1u8; 32]);
        let mut deps = setup(&pubkey_hex, 10, &cs);

        let sudo_msg = SudoMsg::UpdateState {
            client_message: Binary::new(to_json_vec(&header).unwrap()),
        };
        assert!(sudo(deps.as_mut(), mock_env(), sudo_msg).is_err());
    }

    #[test]
    fn misbehaviour_freezes_client() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header_a, pubkey_hex) = make_header(12, 0, [1u8; 32]);
        // Same round (epoch 0, view 12), different payload → equivocation.
        let (header_b, _) = make_header(12, 0, [2u8; 32]);
        let mut deps = setup(&pubkey_hex, 10, &cs);

        let mis = Misbehaviour {
            header_a: header_a.clone(),
            header_b: header_b.clone(),
        };
        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::UpdateStateOnMisbehaviour {
                client_message: Binary::new(to_json_vec(&mis).unwrap()),
            },
        )
        .unwrap();
        assert!(res.data.is_none());

        let client = CLIENT_STATE.load(&deps.storage).unwrap();
        assert_eq!(client.frozen_height.unwrap().revision_height, 12);

        // Status query now reports Frozen.
        let status: StatusResponse = from_json(
            query(deps.as_ref(), mock_env(), QueryMsg::Status {}).unwrap(),
        )
        .unwrap();
        assert_eq!(status.status, "Frozen");

        // Further updates are rejected.
        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::UpdateState {
                client_message: Binary::new(to_json_vec(&header_a).unwrap()),
            },
        );
        assert!(matches!(
            res,
            Err(ContractError::ClientFrozen(12))
        ));
    }

    #[test]
    fn timestamp_at_height_returns_consensus_state() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 1_234_567_890,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (_header, pubkey_hex) = make_header(5, 0, [0u8; 32]);
        let deps = setup(&pubkey_hex, 5, &cs);

        let ts: TimestampAtHeightResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::TimestampAtHeight {
                    height: Height::from_block_height(5),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(ts.timestamp, 1_234_567_890);
    }

    #[test]
    fn verify_client_message_dry_run() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header, pubkey_hex) = make_header(20, 0, [4u8; 32]);
        let deps = setup(&pubkey_hex, 10, &cs);

        // Valid header passes without mutating state.
        query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::VerifyClientMessage {
                client_message: Binary::new(to_json_vec(&header).unwrap()),
            },
        )
        .unwrap();

        // Tampered payload fails.
        let mut tampered = header.clone();
        let mut bytes = tampered.proposal_bytes.to_vec();
        let len = bytes.len();
        bytes[len - 32..].copy_from_slice(&[8u8; 32]);
        tampered.proposal_bytes = Binary::new(bytes);
        assert!(query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::VerifyClientMessage {
                client_message: Binary::new(to_json_vec(&tampered).unwrap()),
            },
        )
        .is_err());
    }

    #[test]
    fn check_for_misbehaviour_query() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (header_a, pubkey_hex) = make_header(13, 0, [1u8; 32]);
        let (header_b, _) = make_header(13, 0, [2u8; 32]);
        let deps = setup(&pubkey_hex, 10, &cs);

        let mis = Misbehaviour { header_a, header_b };
        let res: CheckForMisbehaviourResponse = from_json(
            query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::CheckForMisbehaviour {
                    client_message: Binary::new(to_json_vec(&mis).unwrap()),
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert!(res.found_misbehaviour);
    }

    /// Build a 3-leaf state tree and a signed payload committing to its
    /// root. Returns (payload_bytes, proof for k1 at index 1, k1, v1).
    ///
    /// Tree:  N0 = H(0x01||l0||l1); l2 promotes; root = H(0x01||N0||l2).
    /// Proof for index 1: siblings = [l0, l2].
    fn membership_fixture() -> (Binary, MembershipProof, Vec<u8>, Vec<u8>) {
        let k0 = b"commitments/ports/transfer/channels/channel-0/sequences/0".to_vec();
        let k1 = b"commitments/ports/transfer/channels/channel-0/sequences/1".to_vec();
        let k2 = b"commitments/ports/transfer/channels/channel-0/sequences/2".to_vec();
        let v1 = b"packet-commitment-1".to_vec();
        let l0 = merkle::leaf_hash(&k0, b"packet-commitment-0");
        let l1 = merkle::leaf_hash(&k1, &v1);
        let l2 = merkle::leaf_hash(&k2, b"packet-commitment-2");

        let mut h = Sha256::new();
        h.update([0x01u8]);
        h.update(l0);
        h.update(l1);
        let n0: [u8; 32] = h.finalize().into();
        let mut h = Sha256::new();
        h.update([0x01u8]);
        h.update(n0);
        h.update(l2);
        let root: [u8; 32] = h.finalize().into();

        let payload = BlockPayloadMirror {
            height: 7,
            timestamp_nanos: 42,
            proposer: vec![9u8; 32],
            txs: vec![vec![1, 2, 3]],
            parent_digest: [5u8; 32],
            state_root: root,
        };
        let payload_bytes = Binary::new(bincode::serialize(&payload).unwrap());
        let proof = MembershipProof {
            payload_bytes: payload_bytes.clone(),
            leaf_index: 1,
            siblings: vec![Some(Binary::from(l0)), Some(Binary::from(l2))],
        };
        (payload_bytes, proof, k1, v1)
    }

    #[test]
    fn verify_membership_valid_proof() {
        let (payload_bytes, proof, key, value) = membership_fixture();
        let digest = Sha256::digest(payload_bytes.as_slice());
        let cs = ConsensusState {
            payload_digest_hex: hex::encode(digest),
            timestamp: 42,
            epoch: 0,
            view: 7,
            parent: 6,
        };
        let (_header, pubkey_hex) = make_header(7, 42, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 7, &cs);

        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::VerifyMembership {
                height: Height::from_block_height(7),
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::new(to_json_vec(&proof).unwrap()),
                merkle_path: MerklePath {
                    key_path: vec![Binary::from(key)],
                },
                value: Binary::from(value),
            },
        );
        assert!(res.is_ok(), "valid proof rejected: {:?}", res.err());
    }

    #[test]
    fn verify_membership_wrong_value_fails() {
        let (payload_bytes, proof, key, _value) = membership_fixture();
        let digest = Sha256::digest(payload_bytes.as_slice());
        let cs = ConsensusState {
            payload_digest_hex: hex::encode(digest),
            timestamp: 42,
            epoch: 0,
            view: 7,
            parent: 6,
        };
        let (_header, pubkey_hex) = make_header(7, 42, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 7, &cs);

        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::VerifyMembership {
                height: Height::from_block_height(7),
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::new(to_json_vec(&proof).unwrap()),
                merkle_path: MerklePath {
                    key_path: vec![Binary::from(key)],
                },
                value: Binary::from(b"forged-commitment".to_vec()),
            },
        );
        assert!(matches!(res, Err(ContractError::InvalidMembershipProof)));
    }

    #[test]
    fn verify_membership_digest_mismatch_fails() {
        let (_payload_bytes, proof, key, value) = membership_fixture();
        // Consensus state commits to a DIFFERENT payload digest — the proof's
        // payload_bytes won't match, so the binding check must reject.
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0xdeu8; 32]),
            timestamp: 42,
            epoch: 0,
            view: 7,
            parent: 6,
        };
        let (_header, pubkey_hex) = make_header(7, 42, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 7, &cs);

        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::VerifyMembership {
                height: Height::from_block_height(7),
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::new(to_json_vec(&proof).unwrap()),
                merkle_path: MerklePath {
                    key_path: vec![Binary::from(key)],
                },
                value: Binary::from(value),
            },
        );
        assert!(matches!(res, Err(ContractError::InvalidMembershipProof)));
    }

    #[test]
    fn verify_membership_missing_consensus_fails() {
        let (_payload_bytes, proof, key, value) = membership_fixture();
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (_header, pubkey_hex) = make_header(6, 0, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 6, &cs);

        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::VerifyMembership {
                height: Height::from_block_height(99),
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::new(to_json_vec(&proof).unwrap()),
                merkle_path: MerklePath {
                    key_path: vec![Binary::from(key)],
                },
                value: Binary::from(value),
            },
        );
        assert!(matches!(res, Err(ContractError::ConsensusStateNotFound(99))));
    }

    #[test]
    fn non_membership_unsupported_returns_error() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (_header, pubkey_hex) = make_header(6, 0, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 6, &cs);

        let res = sudo(
            deps.as_mut(),
            mock_env(),
            SudoMsg::VerifyNonMembership {
                height: Height::from_block_height(6),
                delay_time_period: 0,
                delay_block_period: 0,
                proof: Binary::default(),
                merkle_path: MerklePath {
                    key_path: vec![],
                },
            },
        );
        assert!(matches!(res, Err(ContractError::MembershipProofsUnsupported)));
    }

    #[test]
    fn migrate_client_store_is_noop() {
        let cs = ConsensusState {
            payload_digest_hex: hex::encode([0u8; 32]),
            timestamp: 0,
            epoch: 0,
            view: 0,
            parent: 0,
        };
        let (_header, pubkey_hex) = make_header(6, 0, [0u8; 32]);
        let mut deps = setup(&pubkey_hex, 6, &cs);
        assert!(sudo(deps.as_mut(), mock_env(), SudoMsg::MigrateClientStore {}).is_ok());
    }
}
