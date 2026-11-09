//! `cancel` and `expire`.

extern crate std;

use soroban_sdk::{testutils::Address as _, Address};

use super::helpers::{err, Fosay, MAX_PENDING, OWNER_RULE, UNIT};
use crate::{errors::ApprovalsError, types::IntentStatus};

#[test]
fn owner_rejects_and_agent_withdraws() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let a = f.agent_propose(&to, 10 * UNIT);
    let b = f.agent_propose(&to, 10 * UNIT);
    assert!(f.try_cancel(&[&f.owner], OWNER_RULE, a));
    assert!(f.try_cancel(&[&f.agent], f.ask_rule, b));
    assert_eq!(f.q().get_intent(&a).status, IntentStatus::Cancelled);
    assert_eq!(f.q().get_intent(&b).status, IntentStatus::Cancelled);
    assert_eq!(f.q().pending_count(&f.account), 0);
    // a cancelled request can't be executed or cancelled again
    assert!(!f.owner_execute(a));
    assert!(!f.try_cancel(&[&f.owner], OWNER_RULE, a));
    assert!(!f.try_cancel(&[&f.stranger], f.ask_rule, b));
    assert_eq!(f.balance(&to), 0);
}

#[test]
fn cancel_errors() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);
    f.e.mock_all_auths();
    f.q().cancel(&id);
    assert_eq!(
        f.q().try_cancel(&id).unwrap_err(),
        err(ApprovalsError::NotPending)
    );
    assert_eq!(
        f.q().try_cancel(&77).unwrap_err(),
        err(ApprovalsError::NotFound)
    );
}

#[test]
fn expire_only_after_expiry_and_frees_slot() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&f.e);
    let mut ids = std::vec::Vec::new();
    for _ in 0..MAX_PENDING {
        ids.push(f.agent_propose(&to, 10 * UNIT)); // ttl 500
    }
    assert!(f.try_agent_propose(&to, UNIT, 10).is_err());
    // no auth needed for expire
    assert_eq!(
        f.q().try_expire(&ids[0]).unwrap_err(),
        err(ApprovalsError::NotYetExpired)
    );
    f.advance(500);
    assert_eq!(
        f.q().try_expire(&ids[0]).unwrap_err(),
        err(ApprovalsError::NotYetExpired)
    );
    f.advance(1);
    f.q().expire(&ids[0]);
    assert_eq!(f.q().get_intent(&ids[0]).status, IntentStatus::Expired);
    assert_eq!(f.q().pending_count(&f.account), MAX_PENDING - 1);
    assert_eq!(
        f.q().try_expire(&ids[0]).unwrap_err(),
        err(ApprovalsError::NotPending)
    );
    assert!(f.try_agent_propose(&to, UNIT, 10).is_ok());
}
