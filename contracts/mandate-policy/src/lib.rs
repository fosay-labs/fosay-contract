#![no_std]
//! # FOSAY mandate policy
//!
//! An OpenZeppelin smart-account policy installed on an AI agent's `:pay` context
//! rule. See `docs/SPEC.md` §4.

pub mod budget;
pub mod contract;
pub mod errors;
pub mod events;
pub mod storage;
pub mod types;

#[cfg(test)]
mod test;

pub use crate::contract::*;
pub use crate::errors::*;
pub use crate::types::*;
