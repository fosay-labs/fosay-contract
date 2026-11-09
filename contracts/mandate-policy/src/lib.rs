#![no_std]
//! # FOSAY mandate policy
//!
//! An OpenZeppelin smart-account policy installed on an AI agent's `:pay` context
//! rule. See `docs/SPEC.md` §4.

pub mod contract;

pub use crate::contract::*;
