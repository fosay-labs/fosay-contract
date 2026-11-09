//! Parameter validation, rule checks and transfer-context decoding (SPEC §4).

use soroban_sdk::{
    auth::{Context, ContractContext},
    panic_with_error, symbol_short, Address, Env, MuxedAddress, TryFromVal, Vec,
};
use stellar_accounts::smart_account::{ContextRule, ContextRuleType, Signer};

use crate::{
    errors::MandateError,
    types::{MAX_PERIOD_LEDGERS, MAX_RECIPIENTS, MIN_PERIOD_LEDGERS},
};

/// Validates budget, per-tx cap and recipients (`InvalidParams`).
pub fn validate_limits(e: &Env, budget: i128, max_per_tx: i128, recipients: &Vec<Address>) {
    if budget <= 0 || max_per_tx <= 0 || max_per_tx > budget || recipients.len() > MAX_RECIPIENTS {
        panic_with_error!(e, MandateError::InvalidParams)
    }
}

/// Validates the budget period (`InvalidParams`).
pub fn validate_period(e: &Env, period_ledgers: u32) {
    if !(MIN_PERIOD_LEDGERS..=MAX_PERIOD_LEDGERS).contains(&period_ledgers) {
        panic_with_error!(e, MandateError::InvalidParams)
    }
}

/// Returns the token address of a `CallContract` rule (`OnlyCallContractAllowed` otherwise).
pub fn rule_token(e: &Env, rule: &ContextRule) -> Address {
    match &rule.context_type {
        ContextRuleType::CallContract(token) => token.clone(),
        _ => panic_with_error!(e, MandateError::OnlyCallContractAllowed),
    }
}

/// Invariant I3: the rule has at least one signer and every one of them authenticated.
///
/// With policies attached, OZ defers signer validation to the policies, and this is
/// the only policy on the agent's `:pay` rule. A rule with no signers is rejected, so
/// that a misconfigured rule can never let unsigned calls spend.
pub fn require_all_signers(e: &Env, rule: &ContextRule, authenticated: &Vec<Signer>) {
    if rule.signers.is_empty() {
        panic_with_error!(e, MandateError::SignerMissing)
    }
    for s in rule.signers.iter() {
        if !authenticated.contains(&s) {
            panic_with_error!(e, MandateError::SignerMissing)
        }
    }
}

/// A decoded SEP-41 `transfer(from, to, amount)` call.
pub struct Transfer {
    pub to: Address,
    pub amount: i128,
}

/// Decodes `context` as a positive `transfer` on `token`; anything else is `NotAllowed`.
pub fn decode_transfer(e: &Env, context: &Context, token: &Address) -> Transfer {
    let ContractContext {
        contract,
        fn_name,
        args,
    } = match context {
        Context::Contract(c) => c,
        _ => panic_with_error!(e, MandateError::NotAllowed),
    };
    if contract != token || *fn_name != symbol_short!("transfer") || args.len() != 3 {
        panic_with_error!(e, MandateError::NotAllowed)
    }
    let to = args
        .get(1)
        .and_then(|v| MuxedAddress::try_from_val(e, &v).ok())
        .unwrap_or_else(|| panic_with_error!(e, MandateError::NotAllowed))
        .address();
    let amount = args
        .get(2)
        .and_then(|v| i128::try_from_val(e, &v).ok())
        .unwrap_or_else(|| panic_with_error!(e, MandateError::NotAllowed));
    if amount <= 0 {
        panic_with_error!(e, MandateError::NotAllowed)
    }
    Transfer { to, amount }
}
