---
name: Wave task
about: A scoped task for a Stellar Wave contributor (maintainers file these)
labels: wave
---

## Summary

One to three sentences: what to build or fix, and what is true when it is done.

## Context

Where this sits today, with `path:line` references. Name the spec or doc sections it touches.

## Acceptance criteria

- [ ]
- [ ]

## Out of scope / do not touch

-

## Tech stack

-

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR.

## Complexity

`tier/trivial` · `tier/medium` · `tier/high`. Claim it by commenting "I'll take this". A
claim with no activity for 7 days goes back to the pool.
