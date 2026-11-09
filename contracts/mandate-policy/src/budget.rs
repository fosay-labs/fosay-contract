//! Fixed-size rolling budget (SPEC §4 "Rolling budget").
//!
//! The window of `period_ledgers` is split into `BUCKETS` buckets of
//! `bucket_len = ceil(period / BUCKETS)` ledgers. Spend is kept in a ring of exactly
//! `SLOTS = BUCKETS + 1` slots, created at install. Bucket index `idx` lives in slot
//! `idx % SLOTS`. The live window is buckets `idx - BUCKETS ..= idx`: `SLOTS`
//! consecutive indices, so they occupy distinct slots. Slots holding an older index
//! are stale; they're ignored when summing and overwritten when written.
//!
//! Any `period_ledgers`-long interval ending at the current ledger lies inside the live
//! window, so enforcing `spent + amount <= budget` guarantees that no rolling window of
//! `period_ledgers` ever exceeds `budget`. The window may be up to one `bucket_len`
//! stricter than the nominal period, never looser.
//!
//! The ring never changes length, so the stored entry's byte size is constant and a
//! payment never pays rent for growing storage (testnet measurement: growing the entry
//! made the first payment cost ~54k stroops vs ~19k in steady state).
//!
//! These functions touch no storage. Arithmetic returns `None` on overflow.

use soroban_sdk::{Env, Vec};

use crate::types::{Bucket, BUCKETS};

/// Number of ring slots: the current bucket plus the previous `BUCKETS`.
pub const SLOTS: u32 = BUCKETS + 1;

/// Ledgers per bucket. `period` is validated to be at least `BUCKETS`, so this is >= 1.
pub fn bucket_len(period: u32) -> u32 {
    period.div_ceil(BUCKETS)
}

/// Index of the bucket that contains `ledger`.
pub fn current_index(ledger: u32, period: u32) -> u32 {
    ledger / bucket_len(period)
}

/// A ring of `SLOTS` zeroed slots, created at install.
pub fn empty_ring(e: &Env) -> Vec<Bucket> {
    let mut ring = Vec::new(e);
    for _ in 0..SLOTS {
        ring.push_back(Bucket {
            index: 0,
            amount: 0,
        });
    }
    ring
}

/// Total spent in the live window ending at bucket `idx`, or `None` on overflow.
pub fn spent(ring: &Vec<Bucket>, idx: u32) -> Option<i128> {
    let min = idx.saturating_sub(BUCKETS);
    let mut total: i128 = 0;
    for b in ring.iter() {
        if b.index >= min && b.index <= idx {
            total = total.checked_add(b.amount)?;
        }
    }
    Some(total)
}

/// Adds `amount` to bucket `idx`, overwriting its slot if it held an older bucket.
/// Returns `None` on overflow or if `ring` is not a `SLOTS`-long ring.
pub fn record(ring: &Vec<Bucket>, idx: u32, amount: i128) -> Option<Vec<Bucket>> {
    if ring.len() != SLOTS {
        return None;
    }
    let slot = idx % SLOTS;
    let current = ring.get(slot)?;
    let new_amount = if current.index == idx {
        current.amount.checked_add(amount)?
    } else {
        amount
    };
    let mut out = ring.clone();
    out.set(
        slot,
        Bucket {
            index: idx,
            amount: new_amount,
        },
    );
    Some(out)
}
