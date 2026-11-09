//! Contract entry points (SPEC §5 "Functions").

use soroban_sdk::{contract, contractimpl, panic_with_error, Env};

use crate::{errors::ApprovalsError, storage, types::Config};

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
}
