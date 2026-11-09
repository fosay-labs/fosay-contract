//! Error codes (SPEC §5 "Errors", range 4200–4299).

use soroban_sdk::contracterror;

/// Errors returned by the approvals queue.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ApprovalsError {
    /// No intent with this id.
    NotFound = 4200,
    /// The intent is not pending.
    NotPending = 4201,
    /// The intent's expiry ledger has passed.
    Expired = 4202,
    /// `expire` was called before the expiry ledger.
    NotYetExpired = 4203,
    /// Amount must be positive.
    InvalidAmount = 4204,
    /// TTL must be in `1..=max_ttl_ledgers`.
    InvalidTtl = 4205,
    /// Memo longer than 140 bytes.
    MemoTooLong = 4206,
    /// The account already has the maximum number of pending intents.
    TooManyPending = 4207,
    /// Constructor arguments must be positive.
    InvalidConfig = 4208,
}
