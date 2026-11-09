#![no_std]
//! Spike S2: moves tokens WITHOUT calling require_auth itself, like
//! fosay-approvals::execute. The payer must authorize the inner transfer.
use soroban_sdk::{contract, contractimpl, token, Address, Env};

#[contract]
pub struct Pay;

#[contractimpl]
impl Pay {
    pub fn pay_from(e: Env, token: Address, from: Address, to: Address, amount: i128) {
        token::TokenClient::new(&e, &token).transfer(&from, &to, &amount);
    }

    /// Variant: require the payer's auth at this level first, so the signed
    /// entry is rooted here (bound to this call) and the transfer is a sub-invocation.
    pub fn pay_rooted(e: Env, token: Address, from: Address, to: Address, amount: i128) {
        from.require_auth();
        token::TokenClient::new(&e, &token).transfer(&from, &to, &amount);
    }
}
