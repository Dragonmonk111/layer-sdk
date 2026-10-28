#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, QueryRequest, Response,
    StdResult, WasmQuery,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use cw_utils::nonpayable;

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, Cw4MemberListResponse, Cw4MemberResponse, Cw4QueryMsg, EvidenceResponse,
    ExecuteMsg, InstantiateMsg, KeyHistoryResponse, QueryMsg, SeatResponse, SeatsResponse,
};
use crate::state::{
    Config, EvidenceRecord, KeyHistoryEntry, SeatRecord, BLS_KEY_OWNER, CONFIG, EVIDENCE,
    HOT_KEY_OWNER, KEY_HISTORY, KEY_HISTORY_SEQ, MAYO_KEY_OWNER, MEMBER_SEAT, NPUB_OWNER, SEATS,
};

// version info for migration info
const CONTRACT_NAME: &str = "seat-attestation";
const CONTRACT_VERSION: &str = "0.1.0";

/// Hash-to-curve domain separation tag for BLS signatures in G1.
/// Matches commonware-cryptography `variant::MinSig::MESSAGE`
/// (primitives::group::G1_MESSAGE) — the variant JunoClaw consensus uses:
/// signatures/partials live in G1 (48 bytes compressed), public keys and
/// threshold share pubkeys live in G2 (96 bytes compressed).
const BLS_SIG_DST: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_POP_";

/// BLS12-381 G2 generator, compressed — the second argument of the
/// MinSig pairing check e(sig_g1, G2_gen) == e(H_g1(msg), share_pk_g2).
const BLS12_381_G2_GENERATOR: [u8; 96] = cosmwasm_std::BLS12_381_G2_GENERATOR;

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;
/// Pages scanned in the list_members fallback before giving up.
const LIST_MEMBERS_MAX_PAGES: usize = 16;
const LIST_MEMBERS_PAGE_LIMIT: u32 = 30;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        cw4_group_addr: deps.api.addr_validate(&msg.cw4_group_addr)?,
        dao_core_addr: deps.api.addr_validate(&msg.dao_core_addr)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("cw4_group_addr", config.cw4_group_addr)
        .add_attribute("dao_core_addr", config.dao_core_addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    match msg {
        ExecuteMsg::UpdateAttestation {
            seat_index,
            hot_ed25519,
            bls_share_pubkey,
            mayo_pubkey,
            npub,
        } => execute_update_attestation(
            deps,
            env,
            info,
            seat_index,
            hot_ed25519,
            bls_share_pubkey,
            mayo_pubkey,
            npub,
        ),
        ExecuteMsg::RevokeAttestation { seat_index } => {
            execute_revoke_attestation(deps, env, info, seat_index)
        }
        ExecuteMsg::PruneSeat { seat_index } => execute_prune_seat(deps, env, info, seat_index),
        ExecuteMsg::UpdateConfig {
            cw4_group_addr,
            dao_core_addr,
        } => execute_update_config(deps, info, cw4_group_addr, dao_core_addr),
        ExecuteMsg::SubmitEvidence {
            seat_index,
            view,
            msg_a,
            sig_a,
            msg_b,
            sig_b,
        } => execute_submit_evidence(deps, env, info, seat_index, view, msg_a, sig_a, msg_b, sig_b),
    }
}

fn execute_update_attestation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    seat_index: u8,
    hot_ed25519: Binary,
    bls_share_pubkey: Binary,
    mayo_pubkey: Option<Binary>,
    npub: Binary,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    ensure!(
        !hot_ed25519.is_empty() && !bls_share_pubkey.is_empty() && !npub.is_empty(),
        ContractError::EmptyKey
    );

    // Sender must hold cw4 weight > 0.
    let weight = member_weight(deps.as_ref(), &config, &info.sender)?;
    ensure!(weight > 0, ContractError::NotMember);

    // If the seat is bound to a *different* member, reject — rebinding
    // requires RevokeAttestation by the bound member or dao_core.
    let existing = SEATS.may_load(deps.storage, seat_index)?;
    if let Some(record) = &existing {
        if record.member_addr != info.sender {
            return Err(ContractError::SeatBoundToOtherMember { seat_index });
        }
    } else {
        // Fresh claim: one member may hold only one seat.
        if let Some(other) = MEMBER_SEAT.may_load(deps.storage, info.sender.clone())? {
            return Err(ContractError::MemberAlreadyBound {
                seat_index: other,
            });
        }
    }

    // Every key field is globally unique across active records — a member
    // cannot bind another seat's npub or hot key.
    ensure_key_free(deps.as_ref(), HOT_KEY_OWNER, &hot_ed25519, seat_index)?;
    ensure_key_free(deps.as_ref(), BLS_KEY_OWNER, &bls_share_pubkey, seat_index)?;
    ensure_key_free(deps.as_ref(), NPUB_OWNER, &npub, seat_index)?;
    if let Some(mayo) = &mayo_pubkey {
        ensure!(!mayo.is_empty(), ContractError::EmptyKey);
        ensure_key_free(deps.as_ref(), MAYO_KEY_OWNER, mayo, seat_index)?;
    }

    let now = env.block.time.seconds();

    // Rotation of own seat: retire the old hot keys into KeyHistory with
    // a validity window so evidence signed under a superseded key still
    // resolves to this member.
    if let Some(old) = existing {
        let seq = KEY_HISTORY_SEQ
            .may_load(deps.storage, seat_index)?
            .unwrap_or_default();
        KEY_HISTORY.save(
            deps.storage,
            (seat_index, seq),
            &KeyHistoryEntry {
                seq,
                hot_ed25519: old.hot_ed25519.clone(),
                npub: old.npub.clone(),
                attested_at: old.attested_at,
                superseded_at: now,
            },
        )?;
        KEY_HISTORY_SEQ.save(deps.storage, seat_index, &(seq + 1))?;
        remove_key_indexes(deps.storage, &old);
    }

    let record = SeatRecord {
        seat_index,
        member_addr: info.sender.clone(),
        hot_ed25519,
        bls_share_pubkey,
        mayo_pubkey,
        npub,
        attested_at: now,
    };
    SEATS.save(deps.storage, seat_index, &record)?;
    MEMBER_SEAT.save(deps.storage, info.sender.clone(), &seat_index)?;
    HOT_KEY_OWNER.save(deps.storage, record.hot_ed25519.to_vec(), &seat_index)?;
    BLS_KEY_OWNER.save(
        deps.storage,
        record.bls_share_pubkey.to_vec(),
        &seat_index,
    )?;
    NPUB_OWNER.save(deps.storage, record.npub.to_vec(), &seat_index)?;
    if let Some(mayo) = &record.mayo_pubkey {
        MAYO_KEY_OWNER.save(deps.storage, mayo.to_vec(), &seat_index)?;
    }

    Ok(Response::new()
        .add_attribute("action", "update_attestation")
        .add_attribute("seat_index", seat_index.to_string())
        .add_attribute("member_addr", info.sender))
}

fn execute_revoke_attestation(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    seat_index: u8,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let record = SEATS
        .may_load(deps.storage, seat_index)?
        .ok_or(ContractError::SeatNotFound { seat_index })?;

    // The bound member, or dao_core (a passed DAO proposal executes as
    // dao_core — an ordinary sender check, not a privileged API).
    ensure!(
        info.sender == record.member_addr || info.sender == config.dao_core_addr,
        ContractError::Unauthorized
    );

    remove_seat(deps.storage, &record);

    Ok(Response::new()
        .add_attribute("action", "revoke_attestation")
        .add_attribute("seat_index", seat_index.to_string())
        .add_attribute("member_addr", record.member_addr))
}

fn execute_prune_seat(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    seat_index: u8,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let record = SEATS
        .may_load(deps.storage, seat_index)?
        .ok_or(ContractError::SeatNotFound { seat_index })?;

    // Permissionless, but only when the bound member no longer holds weight.
    let weight = member_weight(deps.as_ref(), &config, &record.member_addr)?;
    ensure!(weight == 0, ContractError::MemberStillActive { seat_index });

    remove_seat(deps.storage, &record);

    Ok(Response::new()
        .add_attribute("action", "prune_seat")
        .add_attribute("seat_index", seat_index.to_string())
        .add_attribute("member_addr", record.member_addr))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    cw4_group_addr: Option<String>,
    dao_core_addr: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    ensure!(
        info.sender == config.dao_core_addr,
        ContractError::Unauthorized
    );

    if let Some(addr) = cw4_group_addr {
        config.cw4_group_addr = deps.api.addr_validate(&addr)?;
    }
    if let Some(addr) = dao_core_addr {
        config.dao_core_addr = deps.api.addr_validate(&addr)?;
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("cw4_group_addr", config.cw4_group_addr)
        .add_attribute("dao_core_addr", config.dao_core_addr))
}

fn execute_submit_evidence(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    seat_index: u8,
    view: u64,
    msg_a: Binary,
    sig_a: Binary,
    msg_b: Binary,
    sig_b: Binary,
) -> Result<Response, ContractError> {
    // Equivocation at the same view requires two *distinct* messages.
    ensure!(msg_a != msg_b, ContractError::NotEquivocating);

    // Evidence must resolve seat → member; an unbound seat cannot be
    // attributed, so reject it outright.
    let seat = SEATS
        .may_load(deps.storage, seat_index)?
        .ok_or(ContractError::SeatNotFound { seat_index })?;

    // One evidence record per (seat, view); first submission wins.
    ensure!(
        !EVIDENCE.has(deps.storage, (seat_index, view)),
        ContractError::DuplicateEvidence { seat_index, view }
    );

    // JunoClaw orientation (commonware `MinSig` variant): partial
    // signatures are compressed G1 (48B); share pubkeys are compressed
    // G2 (96B). Verification: e(sig, G2_gen) == e(H(msg), share_pk).
    // Submissions whose bytes don't decode/verify are still stored with
    // verified=false so governance can review them — the on-chain check
    // is advisory flagging, not a gate.
    let verified = verify_partial_sig(deps.as_ref(), &msg_a, &sig_a, &seat.bls_share_pubkey)
        && verify_partial_sig(deps.as_ref(), &msg_b, &sig_b, &seat.bls_share_pubkey);

    EVIDENCE.save(
        deps.storage,
        (seat_index, view),
        &EvidenceRecord {
            seat_index,
            member_addr: seat.member_addr.clone(),
            view,
            msg_a,
            sig_a,
            msg_b,
            sig_b,
            verified,
            submitted_by: info.sender,
            submitted_at: env.block.time.seconds(),
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "submit_evidence")
        .add_attribute("seat_index", seat_index.to_string())
        .add_attribute("view", view.to_string())
        .add_attribute("member_addr", seat.member_addr)
        .add_attribute("verified", verified.to_string()))
}

/// Verifies a BLS12-381 partial signature in the chain's MinSig
/// orientation via the VM host functions:
///
///   sig ∈ G1 (48B compressed), share_pk ∈ G2 (96B compressed)
///   H(msg) = hash_to_g1(SHA-256, msg, BLS_SIG_DST)
///   e(sig, G2_gen) == e(H(msg), share_pk)
///
/// `msg` must be the exact byte string the node hashed — i.e. the
/// commonware `union_unique(vote_namespace, vote_message)` preimage.
/// Returns false (rather than erroring) on malformed points so evidence
/// still lands for governance review.
fn verify_partial_sig(deps: Deps, msg: &[u8], sig: &[u8], share_pk: &[u8]) -> bool {
    if sig.len() != 48 || share_pk.len() != 96 {
        return false;
    }
    let Ok(hm) = deps
        .api
        .bls12_381_hash_to_g1(cosmwasm_std::HashFunction::Sha256, msg, BLS_SIG_DST)
    else {
        return false;
    };
    // pairing_equality(ps, qs, r, s) checks prod e(p_i, q_i) == e(r, s)
    // with p ∈ G1, q ∈ G2 — here e(sig, G2_gen) == e(H(msg), share_pk).
    deps.api
        .bls12_381_pairing_equality(sig, &BLS12_381_G2_GENERATOR, &hm, share_pk)
        .unwrap_or(false)
}

/// Removes a seat record and all of its secondary indexes. KeyHistory is
/// deliberately untouched — history is append-only so evidence signed
/// under superseded keys stays attributable.
fn remove_seat(storage: &mut dyn cosmwasm_std::Storage, record: &SeatRecord) {
    SEATS.remove(storage, record.seat_index);
    MEMBER_SEAT.remove(storage, record.member_addr.clone());
    remove_key_indexes(storage, record);
}

fn remove_key_indexes(storage: &mut dyn cosmwasm_std::Storage, record: &SeatRecord) {
    HOT_KEY_OWNER.remove(storage, record.hot_ed25519.to_vec());
    BLS_KEY_OWNER.remove(storage, record.bls_share_pubkey.to_vec());
    NPUB_OWNER.remove(storage, record.npub.to_vec());
    if let Some(mayo) = &record.mayo_pubkey {
        MAYO_KEY_OWNER.remove(storage, mayo.to_vec());
    }
}

fn ensure_key_free(
    deps: Deps,
    index: cw_storage_plus::Map<Vec<u8>, u8>,
    key: &[u8],
    seat_index: u8,
) -> Result<(), ContractError> {
    if let Some(owner) = index.may_load(deps.storage, key.to_vec())? {
        if owner != seat_index {
            return Err(ContractError::KeyInUse { seat_index: owner });
        }
    }
    Ok(())
}

/// cw4 member weight via the group contract: `{"member":{"addr":...}}`
/// first, falling back to a `list_members` scan if the member query is
/// unsupported by the deployed group.
fn member_weight(deps: Deps, config: &Config, addr: &cosmwasm_std::Addr) -> StdResult<u64> {
    let member_query: QueryRequest = QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: config.cw4_group_addr.to_string(),
        msg: to_json_binary(&Cw4QueryMsg::Member {
            addr: addr.to_string(),
        })?,
    });
    match deps.querier.query::<Cw4MemberResponse>(&member_query) {
        Ok(resp) => return Ok(resp.weight.unwrap_or_default()),
        Err(_) => {} // fall through to the list_members scan
    }

    let mut start_after: Option<String> = None;
    for _ in 0..LIST_MEMBERS_MAX_PAGES {
        let list_query: QueryRequest = QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.cw4_group_addr.to_string(),
            msg: to_json_binary(&Cw4QueryMsg::ListMembers {
                start_after: start_after.clone(),
                limit: Some(LIST_MEMBERS_PAGE_LIMIT),
            })?,
        });
        let page: Cw4MemberListResponse = deps.querier.query(&list_query)?;
        if page.members.is_empty() {
            return Ok(0);
        }
        if let Some(member) = page.members.iter().find(|m| m.addr == addr.as_str()) {
            return Ok(member.weight);
        }
        if (page.members.len() as u32) < LIST_MEMBERS_PAGE_LIMIT {
            return Ok(0);
        }
        start_after = Some(page.members.last().expect("non-empty page").addr.clone());
    }
    Ok(0)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetSeat { seat_index } => to_json_binary(&SeatResponse {
            seat: SEATS.may_load(deps.storage, seat_index)?,
        }),
        QueryMsg::GetMember { addr } => {
            let addr = deps.api.addr_validate(&addr)?;
            let seat = match MEMBER_SEAT.may_load(deps.storage, addr)? {
                Some(seat_index) => SEATS.may_load(deps.storage, seat_index)?,
                None => None,
            };
            to_json_binary(&SeatResponse { seat })
        }
        QueryMsg::ListActiveSeats { start_after, limit } => {
            let config = CONFIG.load(deps.storage)?;
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start = start_after.map(Bound::exclusive);
            let mut seats = Vec::with_capacity(limit.min(16));
            for item in SEATS
                .range(deps.storage, start, None, Order::Ascending)
                .take(limit)
            {
                let (_, seat) = item?;
                // Server-side join on cw4 membership — the relay allowlist
                // must not admit members who already lost weight. On group
                // query failure the whole query errors so the relay keeps
                // its last good set (it never fails open).
                if member_weight(deps, &config, &seat.member_addr)? > 0 {
                    seats.push(seat);
                }
            }
            to_json_binary(&SeatsResponse { seats })
        }
        QueryMsg::GetKeyHistory { seat_index } => {
            let history = KEY_HISTORY
                .prefix(seat_index)
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| item.map(|(_, entry)| entry))
                .collect::<StdResult<Vec<KeyHistoryEntry>>>()?;
            to_json_binary(&KeyHistoryResponse { history })
        }
        QueryMsg::ListEvidence { seat_index } => {
            let evidence = match seat_index {
                Some(seat) => EVIDENCE
                    .prefix(seat)
                    .range(deps.storage, None, None, Order::Ascending)
                    .map(|item| item.map(|(_, record)| record))
                    .collect::<StdResult<Vec<EvidenceRecord>>>()?,
                None => EVIDENCE
                    .range(deps.storage, None, None, Order::Ascending)
                    .map(|item| item.map(|(_, record)| record))
                    .collect::<StdResult<Vec<EvidenceRecord>>>()?,
            };
            to_json_binary(&EvidenceResponse { evidence })
        }
        QueryMsg::GetConfig {} => to_json_binary(&ConfigResponse {
            config: CONFIG.load(deps.storage)?,
        }),
    }
}
