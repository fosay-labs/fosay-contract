#![no_std]
//! # FOSAY approvals
//!
//! A queue of payment requests over an agent's limits. The agent proposes; the
//! wallet owner approves with `execute(id)`. Holds no funds, has no admin.
//! See `docs/SPEC.md` §5.

pub mod contract;
pub mod types;

pub use crate::contract::*;
pub use crate::types::*;
