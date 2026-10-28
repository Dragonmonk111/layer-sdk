mod cw4_stub;

use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{Addr, Binary, Empty};
use cw_multi_test::{App, AppBuilder, Contract, ContractWrapper, Executor};

use crate::msg::{
    EvidenceResponse, ExecuteMsg, InstantiateMsg, KeyHistoryResponse, QueryMsg, SeatResponse,
    SeatsResponse,
};
use crate::state::SeatRecord;

const DAO_CORE: &str = "dao_core_contract";
const MEMBER_A: &str = "member_alpha";
const MEMBER_B: &str = "member_beta";
const MEMBER_C: &str = "member_gamma";
const OUTSIDER: &str = "not_a_member";

/// The vendored MockApi performs real bech32 validation (prefix
/// "cosmwasm"), so test actors use deterministic addr_make outputs
/// rather than bare labels — anything the contract passes through
/// addr_validate stays valid.
fn mk(label: &str) -> Addr {
    MockApi::default().addr_make(label)
}

fn cw4_code() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        cw4_stub::execute,
        cw4_stub::instantiate,
        cw4_stub::query,
    ))
}

fn seat_code() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ))
}

struct Fixture {
    app: App,
    seat: Addr,
    cw4: Addr,
}

fn setup(members: Vec<(String, u64)>) -> Fixture {
    let mut app = AppBuilder::new().build(|_router, _api, _storage| {});
    let cw4_id = app.store_code(cw4_code());
    let seat_id = app.store_code(seat_code());
    let admin = mk(DAO_CORE);

    let cw4 = app
        .instantiate_contract(
            cw4_id,
            admin.clone(),
            &cw4_stub::InstantiateMsg { members },
            &[],
            "cw4-group",
            None,
        )
        .unwrap();
    let seat = app
        .instantiate_contract(
            seat_id,
            admin,
            &InstantiateMsg {
                cw4_group_addr: cw4.to_string(),
                dao_core_addr: mk(DAO_CORE).to_string(),
            },
            &[],
            "seat-attestation",
            None,
        )
        .unwrap();
    Fixture { app, seat, cw4 }
}

fn default_members() -> Vec<(String, u64)> {
    vec![
        (mk(MEMBER_A).to_string(), 1),
        (mk(MEMBER_B).to_string(), 1),
        (mk(MEMBER_C).to_string(), 1),
    ]
}

fn attest_msg(seat_index: u8, tag: u8) -> ExecuteMsg {
    ExecuteMsg::UpdateAttestation {
        seat_index,
        hot_ed25519: Binary::from(vec![tag; 32]),
        bls_share_pubkey: Binary::from(vec![tag; 96]),
        mayo_pubkey: Some(Binary::from(vec![tag; 16])),
        npub: Binary::from(vec![tag; 32]),
    }
}

fn update_members(fx: &mut Fixture, members: Vec<(String, u64)>) {
    fx.app
        .execute_contract(
            mk(DAO_CORE),
            fx.cw4.clone(),
            &cw4_stub::ExecuteMsg::UpdateMembers { members },
            &[],
        )
        .unwrap();
}

/// Matches against the full anyhow error chain, not just the outermost
/// context message.
fn err_contains(err: &impl std::fmt::Debug, needle: &str) -> bool {
    format!("{err:?}").contains(needle)
}

fn get_seat(fx: &Fixture, seat_index: u8) -> Option<SeatRecord> {
    fx.app
        .wrap()
        .query_wasm_smart::<SeatResponse>(fx.seat.as_str(), &QueryMsg::GetSeat { seat_index })
        .unwrap()
        .seat
}

#[test]
fn attest_happy_path() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(3, 0xAA),
            &[],
        )
        .unwrap();

    let seat = get_seat(&fx, 3).expect("seat must exist");
    assert_eq!(seat.member_addr, mk(MEMBER_A));
    assert_eq!(seat.hot_ed25519, Binary::from(vec![0xAA; 32]));
    assert!(seat.attested_at > 0);

    // member → seat index works too
    let resp: SeatResponse = fx
        .app
        .wrap()
        .query_wasm_smart(
            fx.seat.as_str(),
            &QueryMsg::GetMember {
                addr: mk(MEMBER_A).to_string(),
            },
        )
        .unwrap();
    assert_eq!(resp.seat.unwrap().seat_index, 3);
}

#[test]
fn non_member_rejected() {
    let mut fx = setup(default_members());
    let err = fx
        .app
        .execute_contract(
            mk(OUTSIDER),
            fx.seat.clone(),
            &attest_msg(0, 0x01),
            &[],
        )
        .unwrap_err();
    assert!(
        err_contains(&err, "holds no weight"),
        "unexpected error: {err}"
    );
}

#[test]
fn seat_collision_rejected() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();

    // A different member cannot steal the seat — last-writer-wins is
    // seat-stealing; rebinding requires a revoke first.
    let err = fx
        .app
        .execute_contract(
            mk(MEMBER_B),
            fx.seat.clone(),
            &attest_msg(1, 0x0B),
            &[],
        )
        .unwrap_err();
    assert!(
        err_contains(&err, "different member"),
        "unexpected error: {err}"
    );
}

#[test]
fn member_holds_one_seat_only() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();

    let err = fx
        .app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(2, 0x0B),
            &[],
        )
        .unwrap_err();
    assert!(
        err_contains(&err, "only one seat"),
        "unexpected error: {err}"
    );
}

#[test]
fn rotation_moves_old_keys_to_history() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();
    let first = get_seat(&fx, 1).unwrap();

    // Same member, same seat, new keys — this is a rotation.
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0B),
            &[],
        )
        .unwrap();

    let seat = get_seat(&fx, 1).unwrap();
    assert_eq!(seat.hot_ed25519, Binary::from(vec![0x0B; 32]));

    let history: KeyHistoryResponse = fx
        .app
        .wrap()
        .query_wasm_smart(fx.seat.as_str(), &QueryMsg::GetKeyHistory { seat_index: 1 })
        .unwrap();
    assert_eq!(history.history.len(), 1);
    let entry = &history.history[0];
    assert_eq!(entry.hot_ed25519, Binary::from(vec![0x0A; 32]));
    assert_eq!(entry.npub, Binary::from(vec![0x0A; 32]));
    assert_eq!(entry.attested_at, first.attested_at);
    assert!(entry.superseded_at >= entry.attested_at);
}

#[test]
fn bound_member_can_revoke() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();

    // An outsider cannot revoke.
    let err = fx
        .app
        .execute_contract(
            mk(MEMBER_B),
            fx.seat.clone(),
            &ExecuteMsg::RevokeAttestation { seat_index: 1 },
            &[],
        )
        .unwrap_err();
    assert!(err_contains(&err, "Unauthorized"));

    // The bound member can.
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &ExecuteMsg::RevokeAttestation { seat_index: 1 },
            &[],
        )
        .unwrap();
    assert!(get_seat(&fx, 1).is_none());
}

#[test]
fn dao_core_can_revoke_then_member_rebinds() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();

    fx.app
        .execute_contract(
            mk(DAO_CORE),
            fx.seat.clone(),
            &ExecuteMsg::RevokeAttestation { seat_index: 1 },
            &[],
        )
        .unwrap();
    assert!(get_seat(&fx, 1).is_none());

    // After the dao_core revoke the seat is free for another member.
    fx.app
        .execute_contract(
            mk(MEMBER_B),
            fx.seat.clone(),
            &attest_msg(1, 0x0B),
            &[],
        )
        .unwrap();
    assert_eq!(
        get_seat(&fx, 1).unwrap().member_addr,
        mk(MEMBER_B)
    );
}

#[test]
fn prune_removed_member_and_refuse_active() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();
    fx.app
        .execute_contract(
            mk(MEMBER_B),
            fx.seat.clone(),
            &attest_msg(2, 0x0B),
            &[],
        )
        .unwrap();

    // Permissionless, but refused while the member still holds weight.
    let err = fx
        .app
        .execute_contract(
            mk(OUTSIDER),
            fx.seat.clone(),
            &ExecuteMsg::PruneSeat { seat_index: 1 },
            &[],
        )
        .unwrap_err();
    assert!(
        err_contains(&err, "still holds cw4 weight"),
        "unexpected error: {err}"
    );

    // Remove MEMBER_A from the group — an arbitrary caller may now prune.
    update_members(&mut fx, vec![(mk(MEMBER_A).to_string(), 0)]);
    fx.app
        .execute_contract(
            mk(OUTSIDER),
            fx.seat.clone(),
            &ExecuteMsg::PruneSeat { seat_index: 1 },
            &[],
        )
        .unwrap();
    assert!(get_seat(&fx, 1).is_none());
    assert!(get_seat(&fx, 2).is_some());
}

#[test]
fn list_active_seats_paginates_and_joins_membership() {
    let mut fx = setup(default_members());
    for (member, seat, tag) in [
        (MEMBER_A, 0u8, 0x10u8),
        (MEMBER_B, 1u8, 0x11u8),
        (MEMBER_C, 2u8, 0x12u8),
    ] {
        fx.app
            .execute_contract(
                mk(member),
                fx.seat.clone(),
                &attest_msg(seat, tag),
                &[],
            )
            .unwrap();
    }

    fn query(app: &App, seat: &Addr, start_after: Option<u8>, limit: Option<u32>) -> Vec<SeatRecord> {
        app.wrap()
            .query_wasm_smart::<SeatsResponse>(
                seat.as_str(),
                &QueryMsg::ListActiveSeats { start_after, limit },
            )
            .unwrap()
            .seats
    }
    let all = query(&fx.app, &fx.seat, None, None);
    assert_eq!(all.len(), 3);
    assert_eq!(
        all.iter().map(|s| s.seat_index).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    // Paginate two at a time.
    let page1 = query(&fx.app, &fx.seat, None, Some(2));
    let page2 = query(&fx.app, &fx.seat, Some(1), Some(2));
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2[0].seat_index, 2);

    // A member that lost weight drops out of the allowlist server-side,
    // even before PruneSeat runs.
    update_members(&mut fx, vec![(mk(MEMBER_B).to_string(), 0)]);
    let active = query(&fx.app, &fx.seat, None, None);
    assert_eq!(
        active.iter().map(|s| s.seat_index).collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn submit_evidence_dedups_and_flags_unverified() {
    let mut fx = setup(default_members());
    fx.app
        .execute_contract(
            mk(MEMBER_A),
            fx.seat.clone(),
            &attest_msg(1, 0x0A),
            &[],
        )
        .unwrap();

    let ev = ExecuteMsg::SubmitEvidence {
        seat_index: 1,
        view: 42,
        msg_a: Binary::from(b"payload-a".to_vec()),
        sig_a: Binary::from(vec![0u8; 48]),
        msg_b: Binary::from(b"payload-b".to_vec()),
        sig_b: Binary::from(vec![0u8; 48]),
    };
    fx.app
        .execute_contract(mk(OUTSIDER), fx.seat.clone(), &ev, &[])
        .unwrap();

    // Identical messages are not equivocation.
    let same = ExecuteMsg::SubmitEvidence {
        seat_index: 1,
        view: 43,
        msg_a: Binary::from(b"payload-a".to_vec()),
        sig_a: Binary::from(vec![0u8; 48]),
        msg_b: Binary::from(b"payload-a".to_vec()),
        sig_b: Binary::from(vec![0u8; 48]),
    };
    let err = fx
        .app
        .execute_contract(mk(OUTSIDER), fx.seat.clone(), &same, &[])
        .unwrap_err();
    assert!(err_contains(&err, "not equivocation"));

    // Second evidence for (seat, view) is rejected — first wins.
    let err = fx
        .app
        .execute_contract(mk(OUTSIDER), fx.seat.clone(), &ev, &[])
        .unwrap_err();
    assert!(
        err_contains(&err, "already submitted"),
        "unexpected error: {err}"
    );

    // Stored with verified=false (all-zero sigs don't verify), attributed.
    let stored: EvidenceResponse = fx
        .app
        .wrap()
        .query_wasm_smart(fx.seat.as_str(), &QueryMsg::ListEvidence { seat_index: None })
        .unwrap();
    assert_eq!(stored.evidence.len(), 1);
    let rec = &stored.evidence[0];
    assert_eq!(rec.member_addr, mk(MEMBER_A));
    assert!(!rec.verified);
}

/// Full positive path: a real MinSig partial signature produced by
/// commonware-cryptography (the same scheme/orientation slay3rd uses)
/// must verify on-chain via the VM bls12_381 host functions, which
/// cw-multi-test delegates to cosmwasm-crypto.
/// unix-only: commonware-cryptography needs NASM to build on MSVC.
#[cfg(unix)]
#[test]
fn submit_evidence_verifies_real_bls_partials() {
    use commonware_codec::Encode;
    use commonware_cryptography::bls12381::primitives::{ops, variant::MinSig};
    use commonware_utils::union_unique;

    let mut rng = rand::rngs::OsRng;
    // A keypair models the share: sk*G2 is the bound bls_share_pubkey.
    let (sk, pk_g2) = ops::keypair::<_, MinSig>(&mut rng);
    let share_pk = pk_g2.encode().to_vec(); // 96-byte compressed G2
    assert_eq!(share_pk.len(), 96);

    // The node signs vote partials under namespaces derived from the base
    // consensus namespace (b"slay3r-consensus-v1" + "_NOTARIZE" etc.).
    let ns = b"slay3r-consensus-v1_NOTARIZE";
    let vote_a = b"proposal-a@view42";
    let vote_b = b"proposal-b@view42";
    let sig_a = ops::sign_message::<MinSig>(&sk, ns, vote_a).encode().to_vec();
    let sig_b = ops::sign_message::<MinSig>(&sk, ns, vote_b).encode().to_vec();
    assert_eq!(sig_a.len(), 48);
    assert_eq!(sig_b.len(), 48);

    // The evidence msg_* fields carry the exact hashed preimage:
    // union_unique(vote_namespace, vote_message).
    let msg_a = union_unique(ns, vote_a);
    let msg_b = union_unique(ns, vote_b);

    let mut fx = setup(default_members());
    let msg = ExecuteMsg::UpdateAttestation {
        seat_index: 1,
        hot_ed25519: Binary::from(vec![0xAA; 32]),
        bls_share_pubkey: Binary::from(share_pk),
        mayo_pubkey: None,
        npub: Binary::from(vec![0xBB; 32]),
    };
    fx.app
        .execute_contract(mk(MEMBER_A), fx.seat.clone(), &msg, &[])
        .unwrap();

    fx.app
        .execute_contract(
            mk(OUTSIDER),
            fx.seat.clone(),
            &ExecuteMsg::SubmitEvidence {
                seat_index: 1,
                view: 42,
                msg_a: Binary::from(msg_a),
                sig_a: Binary::from(sig_a),
                msg_b: Binary::from(msg_b),
                sig_b: Binary::from(sig_b),
            },
            &[],
        )
        .unwrap();

    let stored: EvidenceResponse = fx
        .app
        .wrap()
        .query_wasm_smart(fx.seat.as_str(), &QueryMsg::ListEvidence { seat_index: Some(1) })
        .unwrap();
    assert_eq!(stored.evidence.len(), 1);
    assert!(
        stored.evidence[0].verified,
        "valid MinSig partials must verify against the bound share pubkey"
    );

    // A signature over a *different* message must not verify.
    let sig_wrong = ops::sign_message::<MinSig>(&sk, ns, b"other").encode().to_vec();
    fx.app
        .execute_contract(
            mk(OUTSIDER),
            fx.seat.clone(),
            &ExecuteMsg::SubmitEvidence {
                seat_index: 1,
                view: 43,
                msg_a: Binary::from(union_unique(ns, vote_a)),
                sig_a: Binary::from(sig_wrong),
                msg_b: Binary::from(union_unique(ns, vote_b)),
                sig_b: Binary::from(sig_wrong),
            },
            &[],
        )
        .unwrap();
    let stored: EvidenceResponse = fx
        .app
        .wrap()
        .query_wasm_smart(fx.seat.as_str(), &QueryMsg::ListEvidence { seat_index: Some(1) })
        .unwrap();
    let rec = stored
        .evidence
        .iter()
        .find(|e| e.view == 43)
        .expect("evidence must be stored");
    assert!(!rec.verified, "forged partials must land with verified=false");
}
