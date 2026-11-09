//! Security invariants from SPEC §2, checked through the real account.

extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address,
};

use super::helpers::{contract_fn, to_scval, Wallet, OWNER_RULE, UNIT};

/// I1: the agent key can't pause/unpause or edit its own mandate. No agent rule covers
/// the policy contract, and the owner rule needs the owner's key.
#[test]
fn i1_agent_cannot_administer_its_mandate() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let inv = || {
        contract_fn(
            &w.policy,
            "set_paused",
            std::vec![
                to_scval(&w.e, w.account.clone()),
                to_scval(&w.e, w.pay_rule),
                to_scval(&w.e, true),
            ],
        )
    };
    for rule in [w.pay_rule, OWNER_RULE] {
        let entry = w.signed_entry(&[&w.agent], &[rule], inv());
        w.e.set_auths(&[entry]);
        assert!(w
            .policy_client()
            .try_set_paused(&w.account, &w.pay_rule, &true)
            .is_err());
    }
    w.e.set_auths(&[]);
    assert!(
        !w.policy_client()
            .get_mandate(&w.account, &w.pay_rule)
            .paused
    );
}

/// I3: selecting the agent rule without the agent's signature is rejected by the
/// policy, even though OZ defers signer checks to policies on rules that have them.
#[test]
fn i3_agent_rule_without_agent_signature_is_rejected() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(!w.try_pay(&[], &[w.pay_rule], &to, UNIT));
    assert!(!w.try_pay(&[&w.stranger], &[w.pay_rule], &to, UNIT));
    assert!(!w.try_pay(&[&w.owner], &[w.pay_rule], &to, UNIT));
    assert_eq!(w.balance(&to), 0);
    assert!(w.agent_pay(&to, UNIT));
}

/// I7: a successful agent payment emits exactly one event, the token transfer, so
/// the x402 Stellar facilitator accepts it.
#[test]
fn i7_agent_payment_emits_only_the_transfer_event() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(w.agent_pay(&to, UNIT));
    let all = w.e.events().all();
    assert!(all.filter_by_contract(&w.policy).events().is_empty());
    assert!(all.filter_by_contract(&w.account).events().is_empty());
    let token_events = all.filter_by_contract(&w.token);
    assert_eq!(token_events.events().len(), 1);
    assert_eq!(all.events().len(), 1);
}
