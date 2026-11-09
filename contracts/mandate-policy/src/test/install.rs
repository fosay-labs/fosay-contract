//! `install` through the real account (`add_context_rule`) and directly.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, IntoVal, Map, String, Val, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType};

use super::helpers::{oz_account, oz_signer, params, Wallet, UNIT};
use crate::{
    errors::MandateError,
    types::{MandateParams, MAX_PERIOD_LEDGERS},
};

/// Tries to add a rule of `ctx` type carrying the policy with `p`, under mocked auth.
fn try_add(w: &Wallet, ctx: oz_account::ContextRuleType, p: MandateParams) -> bool {
    w.e.mock_all_auths();
    let mut policies: Map<Address, Val> = Map::new(&w.e);
    policies.set(w.policy.clone(), p.into_val(&w.e));
    let ok = w
        .account_client()
        .try_add_context_rule(
            &ctx,
            &String::from_str(&w.e, "agt2:pay"),
            &None,
            &vec![&w.e, oz_signer(&w.e, &w.verifier, &w.agent)],
            &policies,
        )
        .is_ok();
    w.e.set_auths(&[]);
    ok
}

fn call_contract(w: &Wallet) -> oz_account::ContextRuleType {
    oz_account::ContextRuleType::CallContract(w.token.clone())
}

#[test]
fn install_stores_mandate_and_emits_event() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let m = w.policy_client().get_mandate(&w.account, &w.pay_rule);
    assert_eq!(m.token, w.token);
    assert_eq!(m.budget, 5 * UNIT);
    assert_eq!(m.period_ledgers, 1_000);
    assert_eq!(m.max_per_tx, 2 * UNIT);
    assert!(!m.paused);
    assert!(m.buckets.is_empty());
    assert_eq!(
        w.policy_client().remaining_budget(&w.account, &w.pay_rule),
        5 * UNIT
    );

    // a second rule on the same wallet installs independently and emits one event
    let p = params(&w.e, 3 * UNIT, 500, UNIT);
    assert!(try_add(&w, call_contract(&w), p));
    let evs = w.e.events().all().filter_by_contract(&w.policy);
    assert_eq!(evs.events().len(), 1);
}

#[test]
fn install_rejects_non_call_contract_rules() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let p = params(&w.e, 5 * UNIT, 1_000, 2 * UNIT);
    assert!(!try_add(
        &w,
        oz_account::ContextRuleType::Default,
        p.clone()
    ));
    let wasm_hash = soroban_sdk::BytesN::from_array(&w.e, &[1; 32]);
    assert!(!try_add(
        &w,
        oz_account::ContextRuleType::CreateContract(wasm_hash),
        p
    ));
}

#[test]
fn install_rejects_invalid_params() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let e = &w.e;
    let cases: std::vec::Vec<MandateParams> = std::vec![
        params(e, 0, 1_000, 0),                            // zero budget
        params(e, 5 * UNIT, 1_000, 0),                     // zero per-tx cap
        params(e, 5 * UNIT, 1_000, -1),                    // negative per-tx cap
        params(e, 5 * UNIT, 1_000, 6 * UNIT),              // per-tx cap above budget
        params(e, 5 * UNIT, 7, UNIT),                      // period below MIN
        params(e, 5 * UNIT, MAX_PERIOD_LEDGERS + 1, UNIT), // period above MAX
        MandateParams {
            recipients: Vec::from_iter(e, (0..21).map(|_| Address::generate(e))),
            ..params(e, 5 * UNIT, 1_000, UNIT)
        },
    ];
    for p in cases {
        assert!(!try_add(&w, call_contract(&w), p));
    }
    // boundaries are accepted
    assert!(try_add(&w, call_contract(&w), params(e, UNIT, 8, UNIT)));
    assert!(try_add(
        &w,
        call_contract(&w),
        params(e, UNIT, MAX_PERIOD_LEDGERS, UNIT)
    ));
}

/// Direct calls: double install and the error codes themselves.
#[test]
fn direct_install_errors() {
    let e = Env::default();
    e.mock_all_auths();
    let policy = e.register(crate::contract::MandatePolicy, ());
    let client = crate::contract::MandatePolicyClient::new(&e, &policy);
    let account = Address::generate(&e);
    let token = Address::generate(&e);
    let rule_id = |id: u32, ctx: ContextRuleType| ContextRule {
        id,
        context_type: ctx,
        name: String::from_str(&e, "agt1:pay"),
        signers: Vec::new(&e),
        signer_ids: Vec::new(&e),
        policies: vec![&e, policy.clone()],
        policy_ids: vec![&e, 0],
        valid_until: None,
    };
    let rule = |ctx: ContextRuleType| rule_id(4, ctx.clone());
    let p = params(&e, 5 * UNIT, 1_000, UNIT);
    client.install(
        &p,
        &rule(ContextRuleType::CallContract(token.clone())),
        &account,
    );
    assert_eq!(
        client.try_install(&p, &rule(ContextRuleType::CallContract(token)), &account),
        Err(Ok(soroban_sdk::Error::from(MandateError::AlreadyInstalled)))
    );
    assert_eq!(
        client.try_install(&p, &rule(ContextRuleType::Default), &account),
        Err(Ok(soroban_sdk::Error::from(
            MandateError::OnlyCallContractAllowed
        )))
    );
    let bad = params(&e, 5 * UNIT, 1_000, 6 * UNIT);
    let other = Address::generate(&e);
    assert_eq!(
        client.try_install(
            &bad,
            &rule_id(5, ContextRuleType::CallContract(other)),
            &account
        ),
        Err(Ok(soroban_sdk::Error::from(MandateError::InvalidParams)))
    );
    assert_eq!(
        client.try_get_mandate(&Address::generate(&e), &4),
        Err(Ok(soroban_sdk::Error::from(MandateError::NotInstalled)))
    );
}
