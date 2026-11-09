//! Events (SPEC §5 "Events").

use soroban_sdk::{contractevent, Address, BytesN, Env, String};

use crate::types::Intent;

/// The agent asked for an approval. Triggers the owner's push notification.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentProposed {
    #[topic]
    pub account: Address,
    #[topic]
    pub id: u64,
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    pub mandate_hash: BytesN<32>,
    pub memo: String,
    pub expires_ledger: u32,
}

/// The owner approved and the payment was made.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentExecuted {
    #[topic]
    pub account: Address,
    #[topic]
    pub id: u64,
    pub to: Address,
    pub amount: i128,
}

/// The request was rejected (owner) or withdrawn (agent).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentCancelled {
    #[topic]
    pub account: Address,
    #[topic]
    pub id: u64,
}

/// The request expired unanswered.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentExpired {
    #[topic]
    pub account: Address,
    #[topic]
    pub id: u64,
}

pub fn emit_proposed(e: &Env, i: &Intent) {
    IntentProposed {
        account: i.account.clone(),
        id: i.id,
        token: i.token.clone(),
        to: i.to.clone(),
        amount: i.amount,
        mandate_hash: i.mandate_hash.clone(),
        memo: i.memo.clone(),
        expires_ledger: i.expires_ledger,
    }
    .publish(e);
}

pub fn emit_executed(e: &Env, i: &Intent) {
    IntentExecuted {
        account: i.account.clone(),
        id: i.id,
        to: i.to.clone(),
        amount: i.amount,
    }
    .publish(e);
}

pub fn emit_cancelled(e: &Env, i: &Intent) {
    IntentCancelled {
        account: i.account.clone(),
        id: i.id,
    }
    .publish(e);
}

pub fn emit_expired(e: &Env, i: &Intent) {
    IntentExpired {
        account: i.account.clone(),
        id: i.id,
    }
    .publish(e);
}
