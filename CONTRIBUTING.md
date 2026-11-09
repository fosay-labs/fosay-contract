# Contributing to fosay-contract

Thanks for helping. These contracts decide when an AI agent may spend someone's money, so
changes are reviewed for safety first and features second.

## Before you start

1. Read [`docs/SPEC.md`](docs/SPEC.md) §2 (security invariants I1–I7) and the section for the
   contract you are touching. The spec is binding. If code and spec disagree, say so on the
   issue. Don't silently pick one.
2. Comment on the issue to claim it ("I'll take this"). One open claim per person. If there
   is no activity for 7 days, the issue goes back to the pool. Post a draft PR when you are stuck
   instead of going quiet.
3. Changes to `enforce`, `execute`, authorization, storage layout or TTL logic are
   **security-sensitive**. Open an issue and agree on the approach before writing code.

## Setup

- Rust stable with the `wasm32v1-none` target (pinned in [`rust-toolchain.toml`](rust-toolchain.toml))
- Stellar CLI: `cargo install --locked stellar-cli@30.0.1 --no-default-features` (wasm builds need it; soroban-sdk 28 rejects a plain `cargo build --target wasm32v1-none`)
- Node 22 only for `scripts/` (deploy and testnet smoke test)

## Checks (the same as CI)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Run all four and paste the real output in your PR.

## Rules for changes

- **Tests.** Every behaviour change comes with a test. Authorization tests sign with real
  Ed25519 keys against the canonical OpenZeppelin account in `tests/fixtures/`. Don't replace
  them with `mock_all_auths`. Never skip a test, loosen an assertion or edit a fixture to make
  a case pass.
- **Invariants.** A PR that touches I1–I7 names the invariant in its description and adds or
  updates the test listed for it in [`SECURITY.md`](SECURITY.md#threat-model).
- **I7 (no events in `enforce`).** x402 facilitators reject payments that emit anything but the
  transfer event. `i7_agent_payment_emits_only_the_transfer_event` must stay green.
- **Storage.** Keep the mandate entry a constant size (`mandate_entry_size_is_constant_across_payments`).
  Stored types are append-only.
- **Dependencies.** No new crates without discussion on the issue. `soroban-sdk` and
  `stellar-accounts` are pinned exactly.
- **Fixtures.** `tests/fixtures/*.wasm` are byte-identical to deployed contracts, and CI
  checks their hashes. Don't rebuild or replace them.

## Pull requests

- Branch from `main`: `feat/…`, `fix/…`, `test/…`, `docs/…`, `chore/…`.
- [Conventional Commits](https://www.conventionalcommits.org/) subjects, for example
  `fix(mandate-policy): reject zero-length recipient list on update`.
- One logical change per PR. Don't reformat files outside its scope.
- Fill in the PR template. CI must be green and a maintainer must approve; `main` is protected.
- Update [`CHANGELOG.md`](CHANGELOG.md) under **Unreleased** for anything user-visible.

## Deploying

Only maintainers deploy. `scripts/deploy.mjs` is idempotent (deterministic salts). See
[`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). New contract versions are new deployments (I4).

## Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md).
