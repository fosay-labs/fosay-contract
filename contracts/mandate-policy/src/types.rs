//! Storage types and constants (SPEC §4 "Constants", "Storage").

use soroban_sdk::{contracttype, Address, BytesN, Vec};

/// Number of buckets the rolling budget window is divided into.
pub const BUCKETS: u32 = 8;
/// Maximum number of allowlisted recipients per mandate.
pub const MAX_RECIPIENTS: u32 = 20;
/// Shortest allowed budget period: at least one ledger per bucket.
pub const MIN_PERIOD_LEDGERS: u32 = BUCKETS;
/// Longest allowed budget period: 31 days of ledgers.
pub const MAX_PERIOD_LEDGERS: u32 = 535_680;

/// Parameters the owner sets when installing (or updating) a mandate.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandateParams {
    /// Maximum total spend in any rolling window of `period_ledgers`, in token base units.
    pub budget: i128,
    /// Length of the rolling budget window in ledgers.
    pub period_ledgers: u32,
    /// Maximum amount of a single transfer.
    pub max_per_tx: i128,
    /// Allowed recipients; empty means any recipient.
    pub recipients: Vec<Address>,
    /// SHA-256 of the canonical mandate JSON the owner approved.
    pub mandate_hash: BytesN<32>,
}

/// Amount spent within one bucket of the rolling window.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bucket {
    /// Bucket index: `ledger_sequence / bucket_len`.
    pub index: u32,
    /// Total spent in this bucket.
    pub amount: i128,
}

/// Stored mandate for one `(smart_account, context_rule_id)`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MandateData {
    /// Token contract this mandate covers (the rule's `CallContract` address).
    pub token: Address,
    pub budget: i128,
    /// Fixed at install; changing it requires a new rule.
    pub period_ledgers: u32,
    pub max_per_tx: i128,
    pub recipients: Vec<Address>,
    pub mandate_hash: BytesN<32>,
    pub paused: bool,
    /// Budget ring: exactly `BUCKETS + 1` slots, fixed at install (constant entry size).
    pub buckets: Vec<Bucket>,
}

/// Storage keys.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// `(smart_account, context_rule_id)` -> [`MandateData`], persistent.
    Mandate(Address, u32),
}
