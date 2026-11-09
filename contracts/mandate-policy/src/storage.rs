//! Persistent storage helpers and TTL constants (SPEC §4 "Storage").

use soroban_sdk::{panic_with_error, Address, Env};

use crate::{
    errors::MandateError,
    types::{DataKey, MandateData},
};

pub const DAY_IN_LEDGERS: u32 = 17_280;
/// Entries are extended to 30 days of ledgers.
pub const TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;
/// Extension happens once fewer than 29 days remain.
pub const TTL_THRESHOLD: u32 = TTL_EXTEND_TO - DAY_IN_LEDGERS;

fn key(smart_account: &Address, context_rule_id: u32) -> DataKey {
    DataKey::Mandate(smart_account.clone(), context_rule_id)
}

/// Returns the mandate, if installed.
pub fn load(e: &Env, smart_account: &Address, context_rule_id: u32) -> Option<MandateData> {
    e.storage()
        .persistent()
        .get(&key(smart_account, context_rule_id))
}

/// Returns the mandate or aborts with `NotInstalled`.
pub fn load_or_panic(e: &Env, smart_account: &Address, context_rule_id: u32) -> MandateData {
    load(e, smart_account, context_rule_id)
        .unwrap_or_else(|| panic_with_error!(e, MandateError::NotInstalled))
}

/// Writes the mandate without touching its TTL (used by `enforce`, see SPIKES S4).
pub fn save(e: &Env, smart_account: &Address, context_rule_id: u32, data: &MandateData) {
    e.storage()
        .persistent()
        .set(&key(smart_account, context_rule_id), data);
}

/// Writes the mandate and extends its TTL.
pub fn save_and_extend(e: &Env, smart_account: &Address, context_rule_id: u32, data: &MandateData) {
    save(e, smart_account, context_rule_id, data);
    extend(e, smart_account, context_rule_id);
}

/// Extends the mandate entry's TTL. The caller ensures it exists.
pub fn extend(e: &Env, smart_account: &Address, context_rule_id: u32) {
    e.storage().persistent().extend_ttl(
        &key(smart_account, context_rule_id),
        TTL_THRESHOLD,
        TTL_EXTEND_TO,
    );
}

/// Whether a mandate is installed.
pub fn has(e: &Env, smart_account: &Address, context_rule_id: u32) -> bool {
    e.storage()
        .persistent()
        .has(&key(smart_account, context_rule_id))
}

/// Removes the mandate (no-op if absent).
pub fn remove(e: &Env, smart_account: &Address, context_rule_id: u32) {
    e.storage()
        .persistent()
        .remove(&key(smart_account, context_rule_id));
}
