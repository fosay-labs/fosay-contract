//! Constant-size rolling budget (SPEC §4 "Rolling budget").
//!
//! The window of `period_ledgers` is split into `BUCKETS` buckets of
//! `bucket_len = ceil(period / BUCKETS)` ledgers. The live window is the current
//! bucket plus the previous `BUCKETS` buckets. Any `period_ledgers`-long interval
//! ending at the current ledger lies inside those `BUCKETS + 1` buckets, so
//! enforcing `spent(live) + amount <= budget` guarantees that no rolling window of
//! `period_ledgers` ever exceeds `budget`. The window may be up to one `bucket_len`
//! stricter than the nominal period, never looser.
//!
//! These functions touch no storage. Arithmetic returns `None` on overflow.

use soroban_sdk::{Env, Vec};

use crate::types::{Bucket, BUCKETS};

/// Ledgers per bucket. `period` is validated to be at least `BUCKETS`, so this is >= 1.
pub fn bucket_len(period: u32) -> u32 {
    period.div_ceil(BUCKETS)
}

/// Index of the bucket that contains `ledger`.
pub fn current_index(ledger: u32, period: u32) -> u32 {
    ledger / bucket_len(period)
}

/// Keeps only buckets inside the live window ending at bucket `idx`.
pub fn prune(e: &Env, buckets: &Vec<Bucket>, idx: u32) -> Vec<Bucket> {
    let min = idx.saturating_sub(BUCKETS);
    let mut live = Vec::new(e);
    for b in buckets.iter() {
        if b.index >= min {
            live.push_back(b);
        }
    }
    live
}

/// Total of all buckets, or `None` on overflow.
pub fn spent(buckets: &Vec<Bucket>) -> Option<i128> {
    let mut total: i128 = 0;
    for b in buckets.iter() {
        total = total.checked_add(b.amount)?;
    }
    Some(total)
}

/// Adds `amount` to bucket `idx` (merging with an existing entry), or `None` on overflow.
/// Callers prune first, so the result has at most `BUCKETS + 1` entries.
pub fn record(e: &Env, buckets: &Vec<Bucket>, idx: u32, amount: i128) -> Option<Vec<Bucket>> {
    let mut out = Vec::new(e);
    let mut merged = false;
    for b in buckets.iter() {
        if b.index == idx {
            out.push_back(Bucket {
                index: idx,
                amount: b.amount.checked_add(amount)?,
            });
            merged = true;
        } else {
            out.push_back(b);
        }
    }
    if !merged {
        out.push_back(Bucket { index: idx, amount });
    }
    Some(out)
}
