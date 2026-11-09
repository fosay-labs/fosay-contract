//! Owner actions with real owner signatures: pause, update, revoke, TTL, views.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, BytesN, Vec,
};

use super::helpers::{contract_fn, to_scval, Wallet, UNIT};
use crate::errors::MandateError;

fn set_paused_inv(w: &Wallet, paused: bool) -> soroban_sdk::xdr::SorobanAuthorizedInvocation {
    contract_fn(
        &w.policy,
        "set_paused",
        std::vec![
            to_scval(&w.e, w.account.clone()),
            to_scval(&w.e, w.pay_rule),
            to_scval(&w.e, paused),
        ],
    )
}

#[test]
fn owner_pauses_and_unpauses_with_passkey_signature() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    w.as_owner(set_paused_inv(&w, true), 1, || {
        w.policy_client().set_paused(&w.account, &w.pay_rule, &true)
    });
    assert_eq!(
        w.e.events()
            .all()
            .filter_by_contract(&w.policy)
            .events()
            .len(),
        1
    );
    assert!(
        w.policy_client()
            .get_mandate(&w.account, &w.pay_rule)
            .paused
    );
    assert!(!w.agent_pay(&to, UNIT));
    w.as_owner(set_paused_inv(&w, false), 1, || {
        w.policy_client()
            .set_paused(&w.account, &w.pay_rule, &false)
    });
    assert!(w.agent_pay(&to, UNIT));
}

#[test]
fn owner_updates_mandate_and_spend_is_preserved() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(w.agent_pay(&to, 2 * UNIT));
    let new_hash = BytesN::from_array(&w.e, &[9; 32]);
    let recipients: Vec<Address> = vec![&w.e, to.clone()];
    let inv = contract_fn(
        &w.policy,
        "update_mandate",
        std::vec![
            to_scval(&w.e, w.account.clone()),
            to_scval(&w.e, w.pay_rule),
            to_scval(&w.e, 3 * UNIT),
            to_scval(&w.e, UNIT),
            to_scval(&w.e, recipients.clone()),
            to_scval(&w.e, new_hash.clone()),
        ],
    );
    w.as_owner(inv, 1, || {
        w.policy_client().update_mandate(
            &w.account,
            &w.pay_rule,
            &(3 * UNIT),
            &UNIT,
            &recipients,
            &new_hash,
        )
    });
    let m = w.policy_client().get_mandate(&w.account, &w.pay_rule);
    assert_eq!(
        (m.budget, m.max_per_tx, m.mandate_hash),
        (3 * UNIT, UNIT, new_hash)
    );
    assert_eq!(m.period_ledgers, 1_000);
    // 2 already spent in the window → 1 left; the new per-tx cap is 1
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        UNIT
    );
    assert!(!w.agent_pay(&to, UNIT + 1));
    assert!(!w.agent_pay(&Address::generate(&w.e), UNIT)); // not allowlisted now
    assert!(w.agent_pay(&to, UNIT));
    assert!(!w.agent_pay(&to, 1));
}

#[test]
fn update_mandate_validates_params() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    w.e.mock_all_auths();
    let r = w.policy_client().try_update_mandate(
        &w.account,
        &w.pay_rule,
        &UNIT,
        &(2 * UNIT),
        &Vec::new(&w.e),
        &BytesN::from_array(&w.e, &[0; 32]),
    );
    assert_eq!(
        r,
        Err(Ok(soroban_sdk::Error::from(MandateError::InvalidParams)))
    );
}

#[test]
fn owner_revokes_agent_by_removing_the_rule() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    let inv = contract_fn(
        &w.account,
        "remove_context_rule",
        std::vec![to_scval(&w.e, w.pay_rule)],
    );
    w.as_owner(inv, 1, || {
        w.account_client().remove_context_rule(&w.pay_rule)
    });
    assert_eq!(
        w.policy_client().try_get_mandate(&w.account, &w.pay_rule),
        Err(Ok(soroban_sdk::Error::from(MandateError::NotInstalled)))
    );
    assert!(!w.agent_pay(&to, UNIT));
    assert_eq!(w.balance(&to), 0);
}

#[test]
fn extend_ttl_needs_no_auth_and_requires_install() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    // no auths set: anyone may extend
    w.policy_client().extend_ttl(&w.account, &w.pay_rule);
    assert_eq!(
        w.policy_client()
            .try_extend_ttl(&w.account, &(w.pay_rule + 7)),
        Err(Ok(soroban_sdk::Error::from(MandateError::NotInstalled)))
    );
}
