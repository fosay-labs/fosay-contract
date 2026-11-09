//! Budget ring unit tests and the rolling-window property test.

extern crate std;

use soroban_sdk::{Env, Vec};

use crate::{
    budget::{bucket_len, current_index, empty_ring, record, spent, SLOTS},
    types::{Bucket, BUCKETS},
};

#[test]
fn bucket_len_rounds_up() {
    assert_eq!(bucket_len(8), 1);
    assert_eq!(bucket_len(9), 2);
    assert_eq!(bucket_len(16), 2);
    assert_eq!(bucket_len(17_280), 2_160);
    assert_eq!(bucket_len(17_281), 2_161);
}

#[test]
fn current_index_divides_by_bucket_len() {
    assert_eq!(current_index(0, 80), 0);
    assert_eq!(current_index(9, 80), 0);
    assert_eq!(current_index(10, 80), 1);
    assert_eq!(current_index(1_000, 80), 100);
}

#[test]
fn empty_ring_has_fixed_zeroed_slots() {
    let e = Env::default();
    let ring = empty_ring(&e);
    assert_eq!(ring.len(), SLOTS);
    assert_eq!(SLOTS, BUCKETS + 1);
    assert_eq!(spent(&ring, 500), Some(0));
}

#[test]
fn record_merges_same_bucket_and_overwrites_stale_slot() {
    let e = Env::default();
    let ring = record(&empty_ring(&e), 20, 10).unwrap();
    let ring = record(&ring, 20, 7).unwrap();
    assert_eq!(
        ring.get(20 % SLOTS).unwrap(),
        Bucket {
            index: 20,
            amount: 17
        }
    );
    assert_eq!(spent(&ring, 20), Some(17));
    // index 29 maps to the same slot as 20 (29 % 9 == 20 % 9) and 20 is stale by then
    let ring = record(&ring, 29, 1).unwrap();
    assert_eq!(
        ring.get(29 % SLOTS).unwrap(),
        Bucket {
            index: 29,
            amount: 1
        }
    );
    assert_eq!(spent(&ring, 29), Some(1));
    assert_eq!(ring.len(), SLOTS);
}

#[test]
fn spent_counts_only_the_live_window() {
    let e = Env::default();
    let mut ring = empty_ring(&e);
    for idx in 10..=18u32 {
        ring = record(&ring, idx, 1).unwrap();
    }
    assert_eq!(spent(&ring, 18), Some(9)); // 10..=18 all live
    assert_eq!(spent(&ring, 19), Some(8)); // 10 left the window
    assert_eq!(spent(&ring, 30), Some(0));
    assert_eq!(spent(&ring, 5), Some(0)); // future buckets never count for an earlier idx
}

#[test]
fn overflow_and_malformed_ring_return_none() {
    let e = Env::default();
    let ring = record(&empty_ring(&e), 3, i128::MAX).unwrap();
    assert_eq!(record(&ring, 3, 1), None);
    let ring = record(&ring, 4, 1).unwrap();
    assert_eq!(spent(&ring, 4), None);
    let short: Vec<Bucket> = Vec::new(&e);
    assert_eq!(record(&short, 1, 1), None);
}

/// Deterministic xorshift so the property test needs no extra dependencies.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Replays the enforce algorithm over random sequences and checks, by brute force over
/// the accepted history, that no `period`-long window ever exceeds `budget`, and that the
/// ring keeps exactly `SLOTS` entries.
#[test]
fn rolling_window_never_exceeds_budget() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for &period in &[8u32, 9, 15, 64, 100, 1_000, 17_280] {
        for _round in 0..20 {
            // fresh env per round: this test does far more host ops than one tx may
            let e = Env::default();
            e.cost_estimate().budget().reset_unlimited();
            let budget: i128 = 1 + rng.below(1_000) as i128;
            let mut ring = empty_ring(&e);
            let mut accepted: std::vec::Vec<(u32, i128)> = std::vec::Vec::new();
            let mut ledger: u32 = 1 + rng.below(1_000) as u32;
            for _step in 0..300 {
                ledger += rng.below(u64::from(period) / 2 + 1) as u32;
                let amount: i128 = 1 + rng.below(budget as u64) as i128;
                let idx = current_index(ledger, period);
                let s = spent(&ring, idx).unwrap();
                if s + amount <= budget {
                    ring = record(&ring, idx, amount).unwrap();
                    accepted.push((ledger, amount));
                }
                assert_eq!(ring.len(), SLOTS);
            }
            for &(end, _) in &accepted {
                let start = end.saturating_sub(period - 1);
                let total: i128 = accepted
                    .iter()
                    .filter(|(l, _)| *l >= start && *l <= end)
                    .map(|(_, a)| *a)
                    .sum();
                assert!(
                    total <= budget,
                    "period {period}: window ending {end} spent {total} > {budget}"
                );
            }
        }
    }
}
