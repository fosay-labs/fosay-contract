//! Contract entry points (SPEC §5 "Functions").

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, String};

use crate::{
    errors::ApprovalsError,
    events, storage,
    types::{Config, Intent, IntentStatus, MAX_MEMO_LEN},
};

/// The FOSAY approvals queue. Holds no funds, has no admin and no upgrade path (I4).
#[contract]
pub struct Approvals;

#[contractimpl]
impl Approvals {
    /// Sets the immutable configuration (SPEC §5 `__constructor`).
    ///
    /// Errors: `InvalidConfig` if either value is zero.
    pub fn __constructor(e: &Env, max_pending_per_account: u32, max_ttl_ledgers: u32) {
        if max_pending_per_account == 0 || max_ttl_ledgers == 0 {
            panic_with_error!(e, ApprovalsError::InvalidConfig)
        }
        storage::set_config(
            e,
            &Config {
                max_pending_per_account,
                max_ttl_ledgers,
            },
        );
        storage::set_next_id(e, 1);
        storage::extend_instance(e);
    }

    /// Opens a payment request (SPEC §5 `propose`): the agent asks for a yes.
    ///
    /// Auth: `account`, satisfied by the agent's `:ask` rule or the owner rule.
    /// Errors: `InvalidAmount`, `InvalidTtl`, `MemoTooLong`, `TooManyPending`.
    /// Events: `IntentProposed`. Returns the new intent id.
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        e: &Env,
        account: Address,
        token: Address,
        to: Address,
        amount: i128,
        mandate_hash: BytesN<32>,
        memo: String,
        ttl_ledgers: u32,
    ) -> u64 {
        account.require_auth();
        storage::extend_instance(e);
        let config = storage::config(e);
        if amount <= 0 {
            panic_with_error!(e, ApprovalsError::InvalidAmount)
        }
        if ttl_ledgers == 0 || ttl_ledgers > config.max_ttl_ledgers {
            panic_with_error!(e, ApprovalsError::InvalidTtl)
        }
        if memo.len() > MAX_MEMO_LEN {
            panic_with_error!(e, ApprovalsError::MemoTooLong)
        }
        let pending = storage::pending_count(e, &account);
        if pending >= config.max_pending_per_account {
            panic_with_error!(e, ApprovalsError::TooManyPending)
        }
        let now = e.ledger().sequence();
        let expires_ledger = now
            .checked_add(ttl_ledgers)
            .unwrap_or_else(|| panic_with_error!(e, ApprovalsError::InvalidTtl));
        let id = storage::next_id(e);
        let next = id
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(e, ApprovalsError::InvalidConfig));
        let intent = Intent {
            id,
            account: account.clone(),
            token,
            to,
            amount,
            mandate_hash,
            memo,
            created_ledger: now,
            expires_ledger,
            status: IntentStatus::Pending,
        };
        storage::save_intent(e, &intent);
        storage::set_pending_count(e, &account, pending + 1);
        storage::set_next_id(e, next);
        events::emit_proposed(e, &intent);
        id
    }
}
