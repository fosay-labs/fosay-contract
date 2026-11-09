//! Events (SPEC §4 "Events"). None of these is ever emitted from `enforce` (invariant I7).

use soroban_sdk::{contractevent, Address, BytesN, Env, Vec};

use crate::types::MandateData;

/// A mandate was installed on an agent rule.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandateInstalled {
    #[topic]
    pub smart_account: Address,
    #[topic]
    pub context_rule_id: u32,
    pub token: Address,
    pub budget: i128,
    pub period_ledgers: u32,
    pub max_per_tx: i128,
    pub recipients: Vec<Address>,
    pub mandate_hash: BytesN<32>,
}

/// The owner changed the budget, cap, allowlist or mandate hash.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandateUpdated {
    #[topic]
    pub smart_account: Address,
    #[topic]
    pub context_rule_id: u32,
    pub budget: i128,
    pub max_per_tx: i128,
    pub recipients: Vec<Address>,
    pub mandate_hash: BytesN<32>,
}

/// The owner paused or unpaused the mandate.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandatePaused {
    #[topic]
    pub smart_account: Address,
    #[topic]
    pub context_rule_id: u32,
    pub paused: bool,
}

/// The mandate was removed (agent revoked).
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandateUninstalled {
    #[topic]
    pub smart_account: Address,
    #[topic]
    pub context_rule_id: u32,
}

pub fn emit_installed(e: &Env, smart_account: &Address, context_rule_id: u32, d: &MandateData) {
    MandateInstalled {
        smart_account: smart_account.clone(),
        context_rule_id,
        token: d.token.clone(),
        budget: d.budget,
        period_ledgers: d.period_ledgers,
        max_per_tx: d.max_per_tx,
        recipients: d.recipients.clone(),
        mandate_hash: d.mandate_hash.clone(),
    }
    .publish(e);
}

pub fn emit_updated(e: &Env, smart_account: &Address, context_rule_id: u32, d: &MandateData) {
    MandateUpdated {
        smart_account: smart_account.clone(),
        context_rule_id,
        budget: d.budget,
        max_per_tx: d.max_per_tx,
        recipients: d.recipients.clone(),
        mandate_hash: d.mandate_hash.clone(),
    }
    .publish(e);
}

pub fn emit_paused(e: &Env, smart_account: &Address, context_rule_id: u32, paused: bool) {
    MandatePaused {
        smart_account: smart_account.clone(),
        context_rule_id,
        paused,
    }
    .publish(e);
}

pub fn emit_uninstalled(e: &Env, smart_account: &Address, context_rule_id: u32) {
    MandateUninstalled {
        smart_account: smart_account.clone(),
        context_rule_id,
    }
    .publish(e);
}
