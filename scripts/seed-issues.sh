#!/usr/bin/env bash
# Creates the labels and the planned Wave issues for fosay-labs/fosay-contract (idempotent:
# existing labels are updated, issues whose title already exists are skipped).
#   gh auth login && bash scripts/seed-issues.sh
set -euo pipefail
REPO=fosay-labs/fosay-contract
# REST only (no GraphQL), so it also runs where GraphQL is unavailable
existing=$(gh api --paginate "repos/$REPO/issues?state=all&per_page=100" --jq ".[].title")
label() { # name color description: create, or update if it exists
  gh api "repos/$REPO/labels" -f name="$1" -f color="$2" -f description="$3" >/dev/null 2>&1 ||
    gh api -X PATCH "repos/$REPO/labels/${1//\//%2F}" -f color="$2" -f description="$3" >/dev/null
}
issue() { # title comma-separated-labels body-file
  if grep -Fxq -- "$1" <<<"$existing"; then echo "skip: $1"; return; fi
  local args=(); IFS=, read -ra ls <<<"$2"; for l in "${ls[@]}"; do args+=(-f "labels[]=$l"); done
  gh api "repos/$REPO/issues" -f title="$1" -F body=@"$3" "${args[@]}" --jq '"#\(.number) \(.title)"'
}
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

label "wave" 7d00ff "Scoped task for Stellar Wave contributors"
label "tier/trivial" c2e0c6 "Wave complexity: trivial (hours)"
label "tier/medium" fbca04 "Wave complexity: medium (a few days)"
label "tier/high" d93f0b "Wave complexity: high (a week or more)"
label "security-sensitive" b60205 "Touches auth, signing, funds or invariants: agree the approach first"
label "testing" 0e8a16 "Tests and test infrastructure"
label "ci" 5319e7 "CI and automation"
label "area/mandate-policy" 1d76db "mandate-policy"
label "area/approvals" 1d76db "approvals"
label "area/scripts" 1d76db "scripts"
label "area/ci" 1d76db "ci"
label "area/docs" 1d76db "docs"

cat > "$tmp/1.md" <<'BODY'
## Summary

`propose` extends every new intent entry to 30 days of TTL, even when the request itself expires in hours. Rent for that TTL is a large part of what `propose` costs. Extend only to the request's expiry plus a grace period.

> **Security-sensitive.** Post your approach on this issue and get a maintainer's OK before writing code.

## Context

- `contracts/approvals/src/storage.rs:11` sets `TTL_EXTEND_TO = 30 * DAY_IN_LEDGERS`, and `set_intent` (`storage.rs:41-47`) uses it for every intent.
- Requests expire at `expires_ledger`, which is at most `max_ttl_ledgers` from the constructor config.
- `expire(id)` and the views must still work until the keeper or anyone calls `expire`, so the entry must outlive `expires_ledger`.

## Acceptance criteria

- [ ] `set_intent` extends to `min(30 days, expires_ledger - now + GRACE)`, with `GRACE` documented (proposal: 1 day).
- [ ] `execute`, `cancel` and `expire` keep working at `expires_ledger` and inside the grace period (tests at the boundary).
- [ ] A test measures the rent or fee of `propose` before and after, and the PR reports both numbers.
- [ ] SPEC §5 (`propose` row and TTL notes) updated.

## Out of scope

- `fosay-mandate-policy` TTL logic.
- The `PendingCount` entry TTL.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/medium`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "feat(approvals): bound intent TTL by the request's expiry to cut propose fees" "wave,tier/medium,area/approvals,enhancement,security-sensitive" "$tmp/1.md"

cat > "$tmp/2.md" <<'BODY'
## Summary

The ring buffer has a property test (`budget::rolling_window_never_exceeds_budget`), but `enforce` as a whole doesn't. Add a proptest that runs random sequences of payments, ledger jumps, `update_mandate`, pause and unpause through the real contract and compares each outcome with a simple reference model.

## Context

- Budget logic: `contracts/mandate-policy/src/budget.rs`. Enforce: `contract.rs` (`impl Policy`).
- The test harness with real signatures: `contracts/mandate-policy/src/test/helpers.rs`.
- The invariants to hold after every step: SPEC §2, plus “spent in any window ≤ budget”.

## Acceptance criteria

- [ ] A proptest (`proptest` as a dev-dependency) runs at least 256 cases of up to 50 operations each.
- [ ] The reference model predicts success or the exact error (`PerTxLimitExceeded`, `BudgetExceeded`, `Paused`, `RecipientNotAllowed`), and the test asserts the contract agrees.
- [ ] Failures shrink to a minimal sequence (default proptest behaviour); no `#[ignore]`.
- [ ] Runtime in CI stays under 2 minutes.

## Out of scope

- Changing contract behaviour. If the model and contract disagree, open an issue with the shrunk case.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/high`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "test(mandate-policy): property-test enforce against a reference model" "wave,tier/high,area/mandate-policy,testing" "$tmp/2.md"

cat > "$tmp/3.md" <<'BODY'
## Summary

Add tests that record CPU instructions and memory for the hot paths and fail if they grow past a set margin, so a change can't quietly push x402 payments over the facilitator's fee ceiling.

## Context

- x402 facilitators reject payments above their fee ceiling (50,000 stroops by default); measured costs are in `docs/DEPLOYMENT.md`.
- soroban-sdk exposes the budget through `env.cost_estimate()` in tests.

## Acceptance criteria

- [ ] Tests cover `enforce` (through an agent payment), `propose`, `execute` and `cancel`.
- [ ] Each test asserts CPU instructions and memory bytes against constants that sit about 10% above today's measurement, and the values are documented in `docs/DEPLOYMENT.md`.
- [ ] CI runs them as part of `cargo test --workspace`.

## Out of scope

- Optimising the contracts in the same PR.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/medium`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "test: budget-metering tests for enforce, propose and execute" "wave,tier/medium,area/mandate-policy,testing" "$tmp/3.md"

cat > "$tmp/4.md" <<'BODY'
## Summary

Check dependencies against the RustSec advisory database and enforce a license allowlist on every PR.

## Context

- CI: `.github/workflows/ci.yml` (one job named `fmt, clippy, test, wasm build`).
- The workspace pins `soroban-sdk` and `stellar-accounts` exactly (`Cargo.toml`).

## Acceptance criteria

- [ ] A separate job runs `cargo audit` and `cargo deny check` (advisories, licenses, bans, sources).
- [ ] `deny.toml` allows Apache-2.0, MIT, BSD-2/3, ISC, Unicode-3.0 and anything else currently required, with a comment for each exception.
- [ ] The job passes on `main`.

## Out of scope

- Changing dependency versions.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/trivial`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "ci: add cargo-audit and cargo-deny to CI" "wave,tier/trivial,area/ci,ci,good first issue" "$tmp/4.md"

cat > "$tmp/5.md" <<'BODY'
## Summary

Make every CI run upload the two release wasm files with a `hashes.txt` (sha256 and byte size), so a deployed contract can be matched to a commit.

## Context

- `stellar contract build` writes `target/wasm32v1-none/release/fosay_mandate_policy.wasm` and `fosay_approvals.wasm`.
- Deployed hashes are recorded in `deployments/testnet.json`.

## Acceptance criteria

- [ ] The CI job uploads both wasm files and `hashes.txt` with `actions/upload-artifact`.
- [ ] The job summary (`$GITHUB_STEP_SUMMARY`) prints a table with name, size and sha256.
- [ ] README's “Quick start” mentions where to find the artifact.

## Out of scope

- Reproducible-build tooling (separate issue).

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/trivial`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "ci: publish built wasm, sizes and sha256 hashes as a CI artifact" "wave,tier/trivial,area/ci,ci,good first issue" "$tmp/5.md"

cat > "$tmp/6.md" <<'BODY'
## Summary

Write `docs/ERRORS.md` listing every contract error code, what triggers it and what a client should do, so app developers don't have to read the Rust source.

## Context

- `contracts/mandate-policy/src/errors.rs` (4100–4110) and `contracts/approvals/src/errors.rs` (4200–4208).
- The SDK maps some of them in `fosay-app/packages/sdk/src/errors.ts`.

## Acceptance criteria

- [ ] A table per contract with code, name, which function returns it and when, and the suggested client action.
- [ ] Linked from the README and SPEC.
- [ ] A test or script fails if an error variant exists in code but not in the doc (a simple grep in CI is fine).

## Out of scope

- Renaming or renumbering errors.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/trivial`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "docs: error code reference for both contracts" "wave,tier/trivial,area/docs,documentation,good first issue" "$tmp/6.md"

cat > "$tmp/7.md" <<'BODY'
## Summary

Record the two decisions most likely to be “fixed” by a well-meaning contributor: `enforce` emits no events (I7), and the budget is a fixed 9-slot ring rather than a growing map. Each needs the evidence behind it.

## Context

- Evidence: `docs/SPIKES.md` (facilitator rejecting the OpenZeppelin spending-limit event; first-payment cost with a growing map) and `docs/DEPLOYMENT.md` (measured costs).
- SPEC §2 (I7) and the budget section.

## Acceptance criteria

- [ ] `docs/adr/0001-no-events-in-enforce.md` and `docs/adr/0002-fixed-budget-ring.md`, each with context, decision, consequences and links to the evidence.
- [ ] SPEC links to both.

## Out of scope

- Changing the design.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/trivial`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "docs: decision records for event-free enforce and the fixed budget ring" "wave,tier/trivial,area/docs,documentation,good first issue" "$tmp/7.md"

cat > "$tmp/8.md" <<'BODY'
## Summary

Fuzz `propose` arguments (amount, TTL, memo length, recipient) and check that every input is either accepted and stored exactly, or rejected with the documented error.

## Context

- Validation: `contracts/approvals/src/contract.rs` (`propose`), errors `InvalidAmount`, `InvalidTtl`, `MemoTooLong`, `TooManyPending`.
- Existing fixed cases: `src/test/propose.rs::propose_validates_inputs`.

## Acceptance criteria

- [ ] A proptest generates amounts (including 0, negative and `i128::MAX`), TTLs around `max_ttl_ledgers`, and memos around the length limit.
- [ ] For accepted inputs, `get_intent` returns exactly what was proposed. For rejected ones, the error matches the rule.
- [ ] `pending_count` never exceeds `max_pending_per_account`.

## Out of scope

- Changing the limits.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/medium`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "test(approvals): property-test propose input validation" "wave,tier/medium,area/approvals,testing" "$tmp/8.md"

cat > "$tmp/9.md" <<'BODY'
## Summary

Add a `--dry-run` mode to `scripts/deploy.mjs` that simulates upload and deploy on any network, prints the deterministic contract IDs and fees, and submits nothing. It's needed before any mainnet deployment.

## Context

- `scripts/deploy.mjs` is idempotent (salt = sha256("fosay:<crate>:<wasmhash>")).
- Mainnet OZ verifier IDs are listed in `docs/SPEC.md`.

## Acceptance criteria

- [ ] `node scripts/deploy.mjs --network mainnet --dry-run` runs without a funded key and prints the predicted IDs and estimated fees.
- [ ] Never submits in dry-run mode, which a unit test (with a mocked RPC) checks.
- [ ] `docs/DEPLOYMENT.md` documents it.

## Out of scope

- Deploying to mainnet.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/medium`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "chore(scripts): mainnet deploy dry run" "wave,tier/medium,area/scripts,enhancement" "$tmp/9.md"

cat > "$tmp/10.md" <<'BODY'
## Summary

The threat model in `SECURITY.md` points at specific tests. Add a CI step that fails when a referenced test is renamed or removed, so the document can't drift from the code.

## Context

- `SECURITY.md` → “Invariants → tests” and “Threats → controls” tables use `module::test_name` references.
- `cargo test -- --list` prints every test path.

## Acceptance criteria

- [ ] A script extracts the test references from `SECURITY.md` and checks each against `cargo test --workspace -- --list`.
- [ ] CI fails with the missing names listed.
- [ ] Documented in CONTRIBUTING.

## Out of scope

- Rewriting SECURITY.md.

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/medium`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "ci: check that every test named in SECURITY.md exists" "wave,tier/medium,area/ci,ci" "$tmp/10.md"

cat > "$tmp/11.md" <<'BODY'
## Summary

Add the OpenSSF Scorecard action and CodeQL (Rust and GitHub Actions) so supply-chain and code-scanning results show up in the Security tab.

## Context

- Existing CI: `.github/workflows/ci.yml`. Workflows should pin actions by commit SHA.

## Acceptance criteria

- [ ] `.github/workflows/scorecard.yml` runs weekly and on pushes to `main`, and publishes results.
- [ ] `.github/workflows/codeql.yml` scans `rust` and `actions`.
- [ ] Scorecard badge added to the README.

## Out of scope

- Fixing every Scorecard finding (follow-up issues).

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/trivial`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "ci: add OpenSSF Scorecard and CodeQL workflows" "wave,tier/trivial,area/ci,ci,good first issue" "$tmp/11.md"

cat > "$tmp/12.md" <<'BODY'
## Summary

All agent-path tests sign with Ed25519. Add tests where the owner's WebAuthn signer and the agent's Ed25519 signer both appear in one transaction (for example, the owner approves while an agent payment is in the same batch). Assert that `enforce` still requires exactly the `:pay` rule's signers (I3).

> **Security-sensitive.** Post your approach on this issue and get a maintainer's OK before writing code.

## Context

- I3 check: `contract.rs` (`enforce`, `SignerMissing`).
- WebAuthn verifier fixture: `tests/fixtures/oz_webauthn_verifier.wasm`; owner passkey tests exist in `src/test/admin.rs`.

## Acceptance criteria

- [ ] A test where only the owner's passkey signs an agent-rule payment fails with `SignerMissing`.
- [ ] A test with both signatures in one transaction succeeds only for the contexts each rule allows.
- [ ] SECURITY.md I3 row lists the new tests.

## Out of scope

- Contract changes, unless the tests find a bug (then stop and report privately).

## Tech stack

Rust, soroban-sdk 28.0.0, stellar-accounts 0.8.0 (OpenZeppelin), Stellar CLI 30

## Verify before opening the PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
stellar contract build
```

Paste the real output in the PR. Read `CONTRIBUTING.md` first.

## Complexity

`tier/high`. Claim it by commenting “I'll take this”. A claim with no activity for 7 days goes back to the pool.
BODY
issue "test(mandate-policy): enforce with a WebAuthn-signed owner context" "wave,tier/high,area/mandate-policy,testing,security-sensitive" "$tmp/12.md"

