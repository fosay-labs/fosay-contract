//! Storage types (SPEC §5 "Storage").

use soroban_sdk::{contracttype, Address, BytesN, String};

/// Lifecycle of a payment request.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntentStatus {
    Pending,
    Executed,
    Cancelled,
    Expired,
}

/// A payment request the agent cannot make on its own.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intent {
    pub id: u64,
    /// The FOSAY smart account that pays.
    pub account: Address,
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    /// The task/mandate this belongs to (may be all zeroes).
    pub mandate_hash: BytesN<32>,
    /// The agent's reason, at most 140 bytes.
    pub memo: String,
    pub created_ledger: u32,
    pub expires_ledger: u32,
    pub status: IntentStatus,
}

/// Immutable configuration set at deployment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub max_pending_per_account: u32,
    pub max_ttl_ledgers: u32,
}

/// Storage keys.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Instance: [`Config`].
    Config,
    /// Instance: next intent id (`u64`, starts at 1).
    NextId,
    /// Persistent: [`Intent`] by id.
    Intent(u64),
    /// Persistent: number of pending intents for an account (`u32`).
    PendingCount(Address),
}

/// Maximum memo length in bytes.
pub const MAX_MEMO_LEN: u32 = 140;
