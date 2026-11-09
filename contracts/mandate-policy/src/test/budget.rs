//! Budget math unit tests and the rolling-window property test.

extern crate std;

use soroban_sdk::{vec, Env, Vec};

use crate::{
    budget::{bucket_len, current_index, prune, record, spent},
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
fn prune_keeps_current_and_previous_buckets() {
    let e = Env::default();
    let b = |index, amount| Bucket { index, amount };
    let buckets = vec![&e, b(1, 1), b(2, 2), b(10, 3), b(11, 4), b(12, 5)];
    // idx 12 keeps indices >= 4
    let live = prune(&e, &buckets, 12);
    assert_eq!(live, vec![&e, b(10, 3), b(11, 4), b(12, 5)]);
    // boundary: idx 10 keeps indices >= 2
    assert_eq!(prune(&e, &buckets, 10).len(), 4);
    // idx below BUCKETS saturates at 0 and keeps everything
    assert_eq!(prune(&e, &buckets, 3).len(), 5);
}

#[test]
fn record_merges_same_bucket_and_appends_new() {
    let e = Env::default();
    let empty: Vec<Bucket> = Vec::new(&e);
    let one = record(&e, &empty, 5, 10).unwrap();
    let merged = record(&e, &one, 5, 7).unwrap();
    assert_eq!(
        merged,
        vec![
            &e,
            Bucket {
                index: 5,
                amount: 17
            }
        ]
    );
    let two = record(&e, &merged, 6, 1).unwrap();
    assert_eq!(two.len(), 2);
    assert_eq!(spent(&two), Some(18));
}

#[test]
fn overflow_returns_none() {
    let e = Env::default();
    let full = vec![
        &e,
        Bucket {
            index: 1,
            amount: i128::MAX,
        },
    ];
    assert_eq!(record(&e, &full, 1, 1), None);
    let two = vec![
        &e,
        Bucket {
            index: 1,
            amount: i128::MAX,
        },
        Bucket {
            index: 2,
            amount: 1,
        },
    ];
    assert_eq!(spent(&two), None);
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
/// the accepted history, that no `period`-long window ever exceeds `budget`. Also checks
/// the bucket list never grows past `BUCKETS + 1`.
#[test]
fn rolling_window_never_exceeds_budget() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for &period in &[8u32, 9, 15, 64, 100, 1_000, 17_280] {
        for _round in 0..20 {
            // fresh env per round: this test does far more host ops than one tx may
            let e = Env::default();
            e.cost_estimate().budget().reset_unlimited();
            let budget: i128 = 1 + rng.below(1_000) as i128;
            let mut buckets: Vec<Bucket> = Vec::new(&e);
            let mut accepted: std::vec::Vec<(u32, i128)> = std::vec::Vec::new();
            let mut ledger: u32 = 1 + rng.below(1_000) as u32;
            for _step in 0..300 {
                ledger += rng.below(u64::from(period) / 2 + 1) as u32;
                let amount: i128 = 1 + rng.below(budget as u64) as i128;
                let idx = current_index(ledger, period);
                let live = prune(&e, &buckets, idx);
                assert!(live.len() <= BUCKETS + 1);
                let s = spent(&live).unwrap();
                if s + amount <= budget {
                    buckets = record(&e, &live, idx, amount).unwrap();
                    assert!(buckets.len() <= BUCKETS + 1);
                    accepted.push((ledger, amount));
                } else {
                    buckets = live;
                }
            }
            // brute force: every window [end - period + 1, end] over accepted spends
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
