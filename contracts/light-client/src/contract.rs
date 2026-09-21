#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, Header, InstantiateMsg, QueryMsg, VerifyHeaderResponse};
use crate::state::{ClientState, ConsensusState, CLIENT_STATE, CONSENSUS_STATES};
use crate::verify::{verify_certificate, DecodedProposal};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate the group public key eagerly so a malformed ceremony output
    // fails at instantiation, not on the first header.
    let pk_bytes = hex::decode(&msg.group_public_key_hex).map_err(|_| ContractError::InvalidPublicKey)?;
    if pk_bytes.len() != 96 {
        return Err(ContractError::InvalidPublicKeyLength(pk_bytes.len()));
    }

    let client_state = ClientState {
        chain_id: msg.chain_id,
        group_public_key_hex: msg.group_public_key_hex,
        latest_height: 0,
        frozen_height: None,
    };
    CLIENT_STATE.save(deps.storage, &client_state)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("chain_id", &client_state.chain_id))
}

/// Runs §6 verification for a header against the current client state.
/// Does not touch storage beyond reading `ClientState` — callers decide
/// whether to persist the result.
fn verify_header(
    deps: Deps,
    client_state: &ClientState,
    header: &Header,
) -> Result<DecodedProposal, ContractError> {
    if let Some(frozen_at) = client_state.frozen_height {
        return Err(ContractError::ClientFrozen(frozen_at));
    }

    let pk_bytes = hex::decode(&client_state.group_public_key_hex).map_err(|_| ContractError::InvalidPublicKey)?;
    let proposal_bytes = hex::decode(&header.proposal_bytes_hex).map_err(|_| ContractError::InvalidProposal)?;
    let certificate_bytes =
        hex::decode(&header.certificate_bytes_hex).map_err(|_| ContractError::InvalidCertificate)?;

    let decoded = verify_certificate(&pk_bytes, &proposal_bytes, &certificate_bytes)?;

    // Consistency check: if we already have a ConsensusState at this height
    // with a different payload digest, this is misbehaviour, not a normal
    // update — reject here and require SubmitMisbehaviour instead.
    if let Some(existing) = CONSENSUS_STATES.may_load(deps.storage, header.height)? {
        let existing_digest = hex::decode(&existing.payload_digest_hex).unwrap_or_default();
        if existing_digest != decoded.payload {
            return Err(ContractError::VerificationFailed);
        }
    }

    Ok(decoded)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateClient { header } => {
            let mut client_state = CLIENT_STATE.load(deps.storage)?;

            if header.height <= client_state.latest_height && client_state.latest_height != 0 {
                return Err(ContractError::NonMonotonicHeight {
                    header: header.height,
                    latest: client_state.latest_height,
                });
            }

            let decoded = verify_header(deps.as_ref(), &client_state, &header)?;

            let consensus_state = ConsensusState {
                payload_digest_hex: hex::encode(decoded.payload),
                timestamp: header.timestamp,
                epoch: decoded.epoch,
                view: decoded.view,
                parent: decoded.parent,
            };
            CONSENSUS_STATES.save(deps.storage, header.height, &consensus_state)?;

            client_state.latest_height = header.height;
            CLIENT_STATE.save(deps.storage, &client_state)?;

            Ok(Response::new()
                .add_attribute("action", "update_client")
                .add_attribute("height", header.height.to_string())
                .add_attribute("payload_digest", consensus_state.payload_digest_hex))
        }
        ExecuteMsg::SubmitMisbehaviour { header_a, header_b } => {
            let mut client_state = CLIENT_STATE.load(deps.storage)?;

            let decoded_a = verify_certificate(
                &hex::decode(&client_state.group_public_key_hex).map_err(|_| ContractError::InvalidPublicKey)?,
                &hex::decode(&header_a.proposal_bytes_hex).map_err(|_| ContractError::InvalidProposal)?,
                &hex::decode(&header_a.certificate_bytes_hex).map_err(|_| ContractError::InvalidCertificate)?,
            )?;
            let decoded_b = verify_certificate(
                &hex::decode(&client_state.group_public_key_hex).map_err(|_| ContractError::InvalidPublicKey)?,
                &hex::decode(&header_b.proposal_bytes_hex).map_err(|_| ContractError::InvalidProposal)?,
                &hex::decode(&header_b.certificate_bytes_hex).map_err(|_| ContractError::InvalidCertificate)?,
            )?;

            let same_round = decoded_a.epoch == decoded_b.epoch && decoded_a.view == decoded_b.view;
            let same_height = header_a.height == header_b.height;
            if !same_round && !same_height {
                return Err(ContractError::MisbehaviourMismatch);
            }
            if decoded_a.payload == decoded_b.payload {
                return Err(ContractError::NoEquivocation);
            }

            let freeze_height = header_a.height.min(header_b.height);
            client_state.frozen_height = Some(freeze_height);
            CLIENT_STATE.save(deps.storage, &client_state)?;

            Ok(Response::new()
                .add_attribute("action", "submit_misbehaviour")
                .add_attribute("frozen_height", freeze_height.to_string()))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, StdError> {
    match msg {
        QueryMsg::ClientState {} => {
            let client_state = CLIENT_STATE.load(deps.storage)?;
            to_json_binary(&client_state)
        }
        QueryMsg::ConsensusState { height } => {
            let consensus_state = CONSENSUS_STATES
                .load(deps.storage, height)
                .map_err(|_| StdError::not_found("ConsensusState"))?;
            to_json_binary(&consensus_state)
        }
        QueryMsg::VerifyHeader { header } => {
            let client_state = CLIENT_STATE.load(deps.storage)?;
            let response = match verify_header(deps, &client_state, &header) {
                Ok(_) => VerifyHeaderResponse {
                    valid: true,
                    reason: None,
                },
                Err(e) => VerifyHeaderResponse {
                    valid: false,
                    reason: Some(e.to_string()),
                },
            };
            to_json_binary(&response)
        }
    }
}
