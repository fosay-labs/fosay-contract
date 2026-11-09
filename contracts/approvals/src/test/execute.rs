//! `execute` through the real account, including invariant I6.

extern crate std;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, AuthorizedFunction, Events},
    token, Address,
};

use super::helpers::{err, Fosay, OWNER_RULE, UNIT};
use crate::{errors::ApprovalsError, types::IntentStatus};

#[test]
fn owner_approves_over_limit_request() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);
    assert!(f.owner_execute(id));
    assert_eq!(
        f.e.events()
            .all()
            .filter_by_contract(&f.approvals)
            .events()
            .len(),
        1
    );
    assert_eq!(f.balance(&to), 10 * UNIT);
    assert_eq!(f.balance(&f.account), 990 * UNIT);
    assert_eq!(f.q().get_intent(&id).status, IntentStatus::Executed);
    assert_eq!(f.q().pending_count(&f.account), 0);
    // the agent's own budget is untouched by owner-approved payments
    assert_eq!(
        f.policy_client().remaining_budget(&f.account, &f.pay_rule),
        5 * UNIT
    );
}

/// I6: the account's authorization is rooted at `execute(id)` with `transfer` nested.
#[test]
fn i6_owner_auth_is_rooted_at_execute() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);
    assert!(f.owner_execute(id));
    let auths = f.e.auths();
    assert_eq!(auths.len(), 1);
    let (addr, root) = &auths[0];
    assert_eq!(addr, &f.account);
    match &root.function {
        AuthorizedFunction::Contract((c, name, _)) => {
            assert_eq!(c, &f.approvals);
            assert_eq!(name, &symbol_short!("execute"));
        }
        _ => panic!("root is not a contract call"),
    }
    assert_eq!(root.sub_invocations.len(), 1);
    match &root.sub_invocations[0].function {
        AuthorizedFunction::Contract((c, name, _)) => {
            assert_eq!(c, &f.token);
            assert_eq!(name, &symbol_short!("transfer"));
        }
        _ => panic!("sub-invocation is not a contract call"),
    }
}

/// I6: an execute-rooted approval can't be replayed as a bare transfer, and a bare
/// transfer approval can't drive `execute`.
#[test]
fn i6_approvals_are_not_interchangeable_with_bare_transfers() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);

    let exec_entry = f.signed_entry(&[&f.owner], &[OWNER_RULE, OWNER_RULE], f.execute_inv(id));
    f.e.set_auths(&[exec_entry]);
    assert!(token::Client::new(&f.e, &f.token)
        .try_transfer(&f.account, &to, &(10 * UNIT))
        .is_err());

    let bare = f.signed_entry(&[&f.owner], &[OWNER_RULE], f.transfer_inv(&to, 10 * UNIT));
    f.e.set_auths(&[bare]);
    assert!(f.q().try_execute(&id).is_err());
    f.e.set_auths(&[]);

    assert_eq!(f.balance(&to), 0);
    assert_eq!(f.q().get_intent(&id).status, IntentStatus::Pending);
}

#[test]
fn agent_cannot_execute_over_limit_but_can_within_limit() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let big = f.agent_propose(&to, 10 * UNIT);
    assert!(!f.agent_execute(big));
    assert_eq!(f.q().get_intent(&big).status, IntentStatus::Pending);
    let small = f.agent_propose(&to, UNIT);
    assert!(f.agent_execute(small));
    assert_eq!(f.balance(&to), UNIT);
    assert_eq!(
        f.policy_client().remaining_budget(&f.account, &f.pay_rule),
        4 * UNIT
    );
}

#[test]
fn owner_needs_one_rule_id_per_context() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);
    assert!(!f.try_execute(&[&f.owner], &[OWNER_RULE], id));
    assert!(!f.try_execute(&[&f.stranger], &[OWNER_RULE, OWNER_RULE], id));
    assert!(f.owner_execute(id));
}

#[test]
fn execute_rejects_expired_repeated_and_unknown() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT); // ttl 500
    let late = f.agent_propose(&to, 10 * UNIT);
    assert!(f.owner_execute(id));
    f.e.mock_all_auths();
    assert_eq!(
        f.q().try_execute(&id).unwrap_err(),
        err(ApprovalsError::NotPending)
    );
    assert_eq!(
        f.q().try_execute(&99).unwrap_err(),
        err(ApprovalsError::NotFound)
    );
    f.advance(501);
    assert_eq!(
        f.q().try_execute(&late).unwrap_err(),
        err(ApprovalsError::Expired)
    );
    f.e.set_auths(&[]);
    assert_eq!(f.balance(&to), 10 * UNIT);
}
