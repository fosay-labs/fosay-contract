//! `enforce` through the real account with real agent signatures.

extern crate std;

use soroban_sdk::{testutils::Address as _, vec, Address};

use super::helpers::{contract_fn, params, to_scval, Wallet, UNIT};
use crate::types::MandateParams;

#[test]
fn payment_within_limits_succeeds_and_records_spend() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(w.agent_pay(&to, 2 * UNIT));
    assert_eq!(w.balance(&to), 2 * UNIT);
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        3 * UNIT
    );
    let m = w.policy_client().get_mandate(&w.account, &w.pay_rule);
    assert_eq!(m.buckets.len(), 1);
}

#[test]
fn payment_over_per_tx_cap_fails() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(!w.agent_pay(&to, 2 * UNIT + 1));
    assert_eq!(w.balance(&to), 0);
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        5 * UNIT
    );
}

#[test]
fn recipient_allowlist_is_enforced() {
    let allowed = std::cell::RefCell::new(None);
    let w = Wallet::with_params(|e| {
        let a = Address::generate(e);
        *allowed.borrow_mut() = Some(a.clone());
        MandateParams {
            recipients: vec![e, a],
            ..params(e, 5 * UNIT, 1_000, 2 * UNIT)
        }
    });
    let allowed = allowed.into_inner().unwrap();
    let other = Address::generate(&w.e);
    assert!(!w.agent_pay(&other, UNIT));
    assert!(w.agent_pay(&allowed, UNIT));
    assert_eq!(w.balance(&allowed), UNIT);
}

#[test]
fn rolling_budget_blocks_then_frees_after_window() {
    // period 1_000 → bucket_len 125; window = current bucket + previous 8
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(w.agent_pay(&to, 2 * UNIT));
    w.advance(10);
    assert!(w.agent_pay(&to, 2 * UNIT));
    assert!(!w.agent_pay(&to, 2 * UNIT)); // 6 > 5
    assert!(w.agent_pay(&to, UNIT)); // exactly 5
    assert!(!w.agent_pay(&to, 1)); // budget exhausted
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        0
    );

    // still inside the window one period later (window is up to one bucket stricter)
    w.advance(1_000);
    assert!(!w.agent_pay(&to, 1));
    // after period + one bucket, everything has left the window
    w.advance(125);
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        5 * UNIT
    );
    assert!(w.agent_pay(&to, 2 * UNIT));
    assert_eq!(w.balance(&to), 7 * UNIT);
}

#[test]
fn paused_mandate_blocks_payments() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    w.e.mock_all_auths();
    w.policy_client().set_paused(&w.account, &w.pay_rule, &true);
    w.e.set_auths(&[]);
    assert!(!w.agent_pay(&to, UNIT));
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        0
    );
    w.e.mock_all_auths();
    w.policy_client()
        .set_paused(&w.account, &w.pay_rule, &false);
    w.e.set_auths(&[]);
    assert!(w.agent_pay(&to, UNIT));
}

#[test]
fn non_transfer_calls_on_the_token_fail() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let spender = Address::generate(&w.e);
    let expiration: u32 = 10_000;
    let inv = contract_fn(
        &w.token,
        "approve",
        std::vec![
            to_scval(&w.e, w.account.clone()),
            to_scval(&w.e, spender.clone()),
            to_scval(&w.e, UNIT),
            to_scval(&w.e, expiration),
        ],
    );
    let entry = w.signed_entry(&[&w.agent], &[w.pay_rule], inv);
    w.e.set_auths(&[entry]);
    assert!(w
        .token_client()
        .try_approve(&w.account, &spender, &UNIT, &expiration)
        .is_err());
    w.e.set_auths(&[]);
    assert_eq!(w.token_client().allowance(&w.account, &spender), 0);
}

#[test]
fn zero_and_negative_amounts_fail() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(!w.agent_pay(&to, 0));
    assert!(!w.agent_pay(&to, -UNIT));
    assert_eq!(w.balance(&w.account), 1_000 * UNIT);
}

#[test]
fn agent_rule_cannot_pay_a_different_token() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let admin = Address::generate(&w.e);
    let other = w.e.register_stellar_asset_contract_v2(admin).address();
    w.e.mock_all_auths();
    soroban_sdk::token::StellarAssetClient::new(&w.e, &other).mint(&w.account, &(10 * UNIT));
    w.e.set_auths(&[]);
    let to = Address::generate(&w.e);
    let inv = contract_fn(
        &other,
        "transfer",
        std::vec![
            to_scval(&w.e, w.account.clone()),
            to_scval(&w.e, to.clone()),
            to_scval(&w.e, UNIT),
        ],
    );
    let entry = w.signed_entry(&[&w.agent], &[w.pay_rule], inv);
    w.e.set_auths(&[entry]);
    assert!(soroban_sdk::token::Client::new(&w.e, &other)
        .try_transfer(&w.account, &to, &UNIT)
        .is_err());
}
