//! `propose` and the constructor.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, String,
};

use super::helpers::{err, zero_hash, Fosay, MAX_PENDING, MAX_TTL, START_LEDGER, UNIT};
use crate::{contract::Approvals, errors::ApprovalsError, types::IntentStatus};

#[test]
fn agent_proposes_and_intent_is_stored() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 9 * UNIT);
    assert_eq!(
        f.e.events()
            .all()
            .filter_by_contract(&f.approvals)
            .events()
            .len(),
        1
    );
    let i = f.q().get_intent(&id);
    assert_eq!(
        (i.id, i.amount, i.status),
        (1, 9 * UNIT, IntentStatus::Pending)
    );
    assert_eq!((&i.account, &i.token, &i.to), (&f.account, &f.token, &to));
    assert_eq!(i.created_ledger, START_LEDGER);
    assert_eq!(i.expires_ledger, START_LEDGER + 500);
    assert_eq!(i.memo, String::from_str(&f.e, "need approval"));
    assert_eq!(f.q().pending_count(&f.account), 1);
    assert_eq!(f.agent_propose(&to, UNIT), 2);
    assert_eq!(f.q().pending_count(&f.account), 2);
}

#[test]
fn propose_validates_inputs() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let h = zero_hash(&f.e);
    let ok_memo = String::from_str(&f.e, "ok");
    f.e.mock_all_auths();
    let q = f.q();
    let t = |amount: i128, memo: &String, ttl: u32| {
        q.try_propose(&f.account, &f.token, &to, &amount, &h, memo, &ttl)
    };
    assert_eq!(
        t(0, &ok_memo, 10).unwrap_err(),
        err(ApprovalsError::InvalidAmount)
    );
    assert_eq!(
        t(-1, &ok_memo, 10).unwrap_err(),
        err(ApprovalsError::InvalidAmount)
    );
    assert_eq!(
        t(UNIT, &ok_memo, 0).unwrap_err(),
        err(ApprovalsError::InvalidTtl)
    );
    assert_eq!(
        t(UNIT, &ok_memo, MAX_TTL + 1).unwrap_err(),
        err(ApprovalsError::InvalidTtl)
    );
    let long = String::from_str(&f.e, &"x".repeat(141));
    assert_eq!(
        t(UNIT, &long, 10).unwrap_err(),
        err(ApprovalsError::MemoTooLong)
    );
    let max = String::from_str(&f.e, &"x".repeat(140));
    assert!(t(UNIT, &max, MAX_TTL).is_ok());
}

#[test]
fn propose_caps_pending_per_account() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    for _ in 0..MAX_PENDING {
        f.agent_propose(&to, UNIT);
    }
    assert!(f.try_agent_propose(&to, UNIT, 10).is_err());
    assert_eq!(f.q().pending_count(&f.account), MAX_PENDING);
}

#[test]
fn stranger_cannot_propose_for_the_wallet() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let memo = String::from_str(&f.e, "x");
    let entry = f.signed_entry(
        &[&f.stranger],
        &[f.ask_rule],
        f.propose_inv(&to, UNIT, &memo, 10),
    );
    f.e.set_auths(&[entry]);
    assert!(f
        .q()
        .try_propose(
            &f.account,
            &f.token,
            &to,
            &UNIT,
            &zero_hash(&f.e),
            &memo,
            &10
        )
        .is_err());
    f.e.set_auths(&[]);
    assert_eq!(f.q().pending_count(&f.account), 0);
}

#[test]
#[should_panic]
fn constructor_rejects_zero_max_pending() {
    let e = Env::default();
    e.register(Approvals, (0u32, 10u32));
}

#[test]
#[should_panic]
fn constructor_rejects_zero_ttl() {
    let e = Env::default();
    e.register(Approvals, (10u32, 0u32));
}

#[test]
fn unknown_intent_is_not_found() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    assert_eq!(
        f.q().try_get_intent(&42).unwrap_err(),
        err(ApprovalsError::NotFound)
    );
}
