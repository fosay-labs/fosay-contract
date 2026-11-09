//! Error codes (SPEC §4 "Errors", range 4100–4199).

use soroban_sdk::contracterror;

/// Errors returned by the mandate policy.
///
/// Inside `__check_auth` the caller only sees `Error(Auth, InvalidAction)`; these
/// codes appear in simulation diagnostics.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MandateError {
    /// No mandate is installed for this account and rule.
    NotInstalled = 4100,
    /// A mandate is already installed for this account and rule.
    AlreadyInstalled = 4101,
    /// The rule is not a `CallContract` rule.
    OnlyCallContractAllowed = 4102,
    /// Budget, period, per-tx cap or recipients are out of range.
    InvalidParams = 4103,
    /// The mandate is paused.
    Paused = 4104,
    /// A signer of the rule did not authenticate (invariant I3).
    SignerMissing = 4105,
    /// The authorized call is not a well-formed positive transfer of the mandate's token.
    NotAllowed = 4106,
    /// The transfer exceeds `max_per_tx`.
    PerTxLimitExceeded = 4107,
    /// The recipient is not on the allowlist.
    RecipientNotAllowed = 4108,
    /// The transfer would exceed the rolling budget.
    BudgetExceeded = 4109,
    /// Arithmetic overflow.
    MathOverflow = 4110,
}
