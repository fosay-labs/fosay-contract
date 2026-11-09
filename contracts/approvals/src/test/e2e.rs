//! End-to-end product flow (SPEC §6) with both FOSAY contracts on one canonical
//! OZ smart account, using real owner and agent signatures throughout.

extern crate std;

use soroban_sdk::{testutils::Address as _, Address};

use super::helpers::{contract_fn, to_scval, Fosay, OWNER_RULE, UNIT};
use crate::types::IntentStatus;

fn pay(f: &Fosay, to: &Address, amount: i128) -> bool {
    let entry = f.signed_entry(&[&f.agent], &[f.pay_rule], f.transfer_inv(to, amount));
    f.e.set_auths(&[entry]);
    let ok = soroban_sdk::token::Client::new(&f.e, &f.token)
        .try_transfer(&f.account, to, &amount)
        .is_ok();
    f.e.set_auths(&[]);
    ok
}

fn owner_call(f: &Fosay, inv: soroban_sdk::xdr::SorobanAuthorizedInvocation, run: impl FnOnce()) {
    let entry = f.signed_entry(&[&f.owner], &[OWNER_RULE], inv);
    f.e.set_auths(&[entry]);
    run();
    f.e.set_auths(&[]);
}

#[test]
fn full_fosay_flow() {
    // budget 5 per 1,000 ledgers, max 2 per payment
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let merchant = Address::generate(&f.e);
    let rem = || f.policy_client().remaining_budget(&f.account, &f.pay_rule);

    // 1. agent pays on its own within the mandate
    assert!(pay(&f, &merchant, 2 * UNIT));
    assert!(pay(&f, &merchant, 2 * UNIT));
    assert_eq!(rem(), UNIT);

    // 2. next payment would exceed the budget, so the agent asks instead
    assert!(!pay(&f, &merchant, 2 * UNIT));
    let id = f.agent_propose(&merchant, 2 * UNIT);

    // 3. owner says yes on their phone
    assert!(f.owner_execute(id));
    assert_eq!(f.q().get_intent(&id).status, IntentStatus::Executed);
    assert_eq!(f.balance(&merchant), 6 * UNIT);

    // 4. agent uses its last unit of budget
    assert!(pay(&f, &merchant, UNIT));
    assert_eq!(rem(), 0);

    // 5. owner freezes the agent; even after the window resets it can't pay
    let pause = |paused: bool| {
        contract_fn(
            &f.policy,
            "set_paused",
            std::vec![
                to_scval(&f.e, f.account.clone()),
                to_scval(&f.e, f.pay_rule),
                to_scval(&f.e, paused),
            ],
            std::vec![],
        )
    };
    owner_call(&f, pause(true), || {
        f.policy_client().set_paused(&f.account, &f.pay_rule, &true)
    });
    f.advance(1_200);
    assert!(!pay(&f, &merchant, UNIT));
    owner_call(&f, pause(false), || {
        f.policy_client()
            .set_paused(&f.account, &f.pay_rule, &false)
    });
    assert_eq!(rem(), 5 * UNIT);
    assert!(pay(&f, &merchant, 2 * UNIT));

    // 6. owner revokes the agent's pay rule: mandate data is uninstalled
    let remove = |rule: u32| {
        contract_fn(
            &f.account,
            "remove_context_rule",
            std::vec![to_scval(&f.e, rule)],
            std::vec![],
        )
    };
    owner_call(&f, remove(f.pay_rule), || {
        f.account_client().remove_context_rule(&f.pay_rule)
    });
    assert!(f
        .policy_client()
        .try_get_mandate(&f.account, &f.pay_rule)
        .is_err());
    assert!(!pay(&f, &merchant, UNIT));

    // the agent can still ask until its :ask rule is revoked too
    assert!(f.try_agent_propose(&merchant, UNIT, 100).is_ok());
    owner_call(&f, remove(f.ask_rule), || {
        f.account_client().remove_context_rule(&f.ask_rule)
    });
    assert!(f.try_agent_propose(&merchant, UNIT, 100).is_err());

    // owner still controls the wallet
    assert_eq!(f.balance(&merchant), 9 * UNIT);
    assert_eq!(f.balance(&f.account), 991 * UNIT);
}
