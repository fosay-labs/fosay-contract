//! Contract entry points (SPEC §5 "Functions").

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, BytesN, Env, String};

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

    /// Approves and pays a pending request (SPEC §5 `execute`): the human says yes.
    ///
    /// Auth (I6): `intent.account.require_auth()` is called **before** the transfer, so
    /// the owner's signed auth entry is rooted at `execute(id)` with the token `transfer`
    /// as its only sub-invocation. That entry can't be replayed as a bare transfer.
    /// The owner signs with `context_rule_ids = [owner, owner]`. An agent can only
    /// execute requests whose nested transfer also passes its `:pay` rule.
    ///
    /// Errors: `NotFound`, `NotPending`, `Expired`. Events: `IntentExecuted`.
    pub fn execute(e: &Env, id: u64) {
        storage::extend_instance(e);
        let mut intent = storage::load_intent(e, id);
        if intent.status != IntentStatus::Pending {
            panic_with_error!(e, ApprovalsError::NotPending)
        }
        if e.ledger().sequence() > intent.expires_ledger {
            panic_with_error!(e, ApprovalsError::Expired)
        }
        intent.account.require_auth();
        intent.status = IntentStatus::Executed;
        storage::save_intent(e, &intent);
        let pending = storage::pending_count(e, &intent.account);
        storage::set_pending_count(e, &intent.account, pending.saturating_sub(1));
        token::TokenClient::new(e, &intent.token).transfer(
            &intent.account,
            &intent.to,
            &intent.amount,
        );
        events::emit_executed(e, &intent);
    }

    /// Rejects (owner) or withdraws (agent) a pending request (SPEC §5 `cancel`).
    ///
    /// Auth: `intent.account`. Errors: `NotFound`, `NotPending`.
    /// Events: `IntentCancelled`.
    pub fn cancel(e: &Env, id: u64) {
        storage::extend_instance(e);
        let mut intent = storage::load_intent(e, id);
        intent.account.require_auth();
        if intent.status != IntentStatus::Pending {
            panic_with_error!(e, ApprovalsError::NotPending)
        }
        Self::close(e, &mut intent, IntentStatus::Cancelled);
        events::emit_cancelled(e, &intent);
    }

    /// Marks an unanswered request as expired and frees its pending slot
    /// (SPEC §5 `expire`). No auth: the FOSAY keeper calls it.
    ///
    /// Errors: `NotFound`, `NotPending`, `NotYetExpired`. Events: `IntentExpired`.
    pub fn expire(e: &Env, id: u64) {
        storage::extend_instance(e);
        let mut intent = storage::load_intent(e, id);
        if intent.status != IntentStatus::Pending {
            panic_with_error!(e, ApprovalsError::NotPending)
        }
        if e.ledger().sequence() <= intent.expires_ledger {
            panic_with_error!(e, ApprovalsError::NotYetExpired)
        }
        Self::close(e, &mut intent, IntentStatus::Expired);
        events::emit_expired(e, &intent);
    }

    /// Returns an intent (SPEC §5 `get_intent`). Errors: `NotFound`.
    pub fn get_intent(e: &Env, id: u64) -> Intent {
        storage::load_intent(e, id)
    }

    /// Number of pending intents for `account` (SPEC §5 `pending_count`).
    pub fn pending_count(e: &Env, account: Address) -> u32 {
        storage::pending_count(e, &account)
    }
}

impl Approvals {
    /// Sets a final status on a pending intent and releases its pending slot.
    fn close(e: &Env, intent: &mut Intent, status: IntentStatus) {
        intent.status = status;
        storage::save_intent(e, intent);
        let pending = storage::pending_count(e, &intent.account);
        storage::set_pending_count(e, &intent.account, pending.saturating_sub(1));
    }
}
