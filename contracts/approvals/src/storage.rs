//! Storage helpers and TTL constants (SPEC §5).

use soroban_sdk::{panic_with_error, Address, Env};

use crate::{
    errors::ApprovalsError,
    types::{Config, DataKey, Intent},
};

pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;
pub const TTL_THRESHOLD: u32 = TTL_EXTEND_TO - DAY_IN_LEDGERS;

/// Extends the instance TTL (Config, NextId).
pub fn extend_instance(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn set_config(e: &Env, config: &Config) {
    e.storage().instance().set(&DataKey::Config, config);
}

/// The config set by the constructor (always present after deployment).
pub fn config(e: &Env) -> Config {
    e.storage()
        .instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic_with_error!(e, ApprovalsError::InvalidConfig))
}

pub fn set_next_id(e: &Env, id: u64) {
    e.storage().instance().set(&DataKey::NextId, &id);
}

pub fn next_id(e: &Env) -> u64 {
    e.storage().instance().get(&DataKey::NextId).unwrap_or(1)
}

/// Writes an intent and extends its TTL.
pub fn save_intent(e: &Env, intent: &Intent) {
    let key = DataKey::Intent(intent.id);
    e.storage().persistent().set(&key, intent);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Returns the intent or aborts with `NotFound`.
pub fn load_intent(e: &Env, id: u64) -> Intent {
    e.storage()
        .persistent()
        .get(&DataKey::Intent(id))
        .unwrap_or_else(|| panic_with_error!(e, ApprovalsError::NotFound))
}

pub fn pending_count(e: &Env, account: &Address) -> u32 {
    e.storage()
        .persistent()
        .get(&DataKey::PendingCount(account.clone()))
        .unwrap_or(0)
}

/// Writes the pending count and extends its TTL.
pub fn set_pending_count(e: &Env, account: &Address, count: u32) {
    let key = DataKey::PendingCount(account.clone());
    e.storage().persistent().set(&key, &count);
    e.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
