//! Contract entry points (SPEC §4 "Functions").

use soroban_sdk::{auth::Context, contract, contractimpl, panic_with_error, Address, Env, Vec};
use stellar_accounts::{
    policies::Policy,
    smart_account::{ContextRule, Signer},
};

use crate::{
    budget,
    errors::MandateError,
    events, storage,
    types::{MandateData, MandateParams},
    validation,
};

/// The FOSAY mandate policy contract. One deployment serves every FOSAY wallet;
/// state is keyed by `(smart_account, context_rule_id)`.
#[contract]
pub struct MandatePolicy;

#[contractimpl]
impl Policy for MandatePolicy {
    type AccountParams = MandateParams;

    /// Enforces the mandate for one authorized call (SPEC §4 `enforce`).
    ///
    /// Auth: `smart_account` (the account calls this from `__check_auth`).
    /// Checks, in order: installed, not paused, all rule signers authenticated (I3),
    /// call is a positive `transfer` of the mandate's token, per-tx cap, recipient
    /// allowlist, rolling budget. Records the spend.
    ///
    /// Emits **no events** (I7) and does **not** extend TTL (SPIKES S4).
    ///
    /// Errors: `NotInstalled`, `Paused`, `SignerMissing`, `NotAllowed`,
    /// `PerTxLimitExceeded`, `RecipientNotAllowed`, `BudgetExceeded`, `MathOverflow`.
    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        smart_account.require_auth();
        let mut data = storage::load_or_panic(e, &smart_account, context_rule.id);
        if data.paused {
            panic_with_error!(e, MandateError::Paused)
        }
        validation::require_all_signers(e, &context_rule, &authenticated_signers);
        let transfer = validation::decode_transfer(e, &context, &data.token);
        if transfer.amount > data.max_per_tx {
            panic_with_error!(e, MandateError::PerTxLimitExceeded)
        }
        if !data.recipients.is_empty() && !data.recipients.contains(&transfer.to) {
            panic_with_error!(e, MandateError::RecipientNotAllowed)
        }
        let idx = budget::current_index(e.ledger().sequence(), data.period_ledgers);
        let live = budget::prune(e, &data.buckets, idx);
        let after = budget::spent(&live)
            .and_then(|s| s.checked_add(transfer.amount))
            .unwrap_or_else(|| panic_with_error!(e, MandateError::MathOverflow));
        if after > data.budget {
            panic_with_error!(e, MandateError::BudgetExceeded)
        }
        data.buckets = budget::record(e, &live, idx, transfer.amount)
            .unwrap_or_else(|| panic_with_error!(e, MandateError::MathOverflow));
        storage::save(e, &smart_account, context_rule.id, &data);
    }

    /// Installs a mandate on a `CallContract(token)` rule (SPEC §4 `install`).
    ///
    /// Auth: `smart_account` (called by the account from `add_context_rule` / `add_policy`).
    /// Errors: `OnlyCallContractAllowed`, `AlreadyInstalled`, `InvalidParams`.
    /// Events: `MandateInstalled`.
    fn install(
        e: &Env,
        install_params: MandateParams,
        context_rule: ContextRule,
        smart_account: Address,
    ) {
        smart_account.require_auth();
        let token = validation::rule_token(e, &context_rule);
        if storage::has(e, &smart_account, context_rule.id) {
            panic_with_error!(e, MandateError::AlreadyInstalled)
        }
        validation::validate_limits(
            e,
            install_params.budget,
            install_params.max_per_tx,
            &install_params.recipients,
        );
        validation::validate_period(e, install_params.period_ledgers);
        let data = MandateData {
            token,
            budget: install_params.budget,
            period_ledgers: install_params.period_ledgers,
            max_per_tx: install_params.max_per_tx,
            recipients: install_params.recipients,
            mandate_hash: install_params.mandate_hash,
            paused: false,
            buckets: Vec::new(e),
        };
        storage::save_and_extend(e, &smart_account, context_rule.id, &data);
        events::emit_installed(e, &smart_account, context_rule.id, &data);
    }

    /// Removes the mandate (SPEC §4 `uninstall`); called when the rule or policy is
    /// removed, i.e. the agent is revoked. No error if nothing is installed.
    ///
    /// Auth: `smart_account`. Events: `MandateUninstalled`.
    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address) {
        smart_account.require_auth();
        storage::remove(e, &smart_account, context_rule.id);
        events::emit_uninstalled(e, &smart_account, context_rule.id);
    }
}
