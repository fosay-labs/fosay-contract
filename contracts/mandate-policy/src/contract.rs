//! Contract entry points (SPEC §4).

use soroban_sdk::contract;

/// The FOSAY mandate policy contract. One deployment serves every FOSAY wallet;
/// state is keyed by `(smart_account, context_rule_id)`.
#[contract]
pub struct MandatePolicy;
