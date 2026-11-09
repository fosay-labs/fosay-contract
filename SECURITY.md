# Security policy

FOSAY contracts sit between an AI agent's key and its owner's funds. Report anything that
lets an agent, a server or a stranger move funds the owner did not allow.

## Status

- **Not audited.** Deployed to Stellar **testnet** only. Do not use with real funds.
- No mainnet deployment exists. Any "FOSAY" contract on mainnet is not ours.

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private reporting:
[Security → Report a vulnerability](https://github.com/fosay-labs/fosay-contract/security/advisories/new).

Include the contract and function, the steps or a failing test, and the impact. A test
against the harness in `contracts/*/src/test/helpers.rs` is the fastest way to a fix.

| Step | Target |
|---|---|
| Acknowledge | 3 days |
| Triage and severity | 7 days |
| Fix or mitigation plan | agreed with the reporter |
| Disclosure | after the fix is deployed, credited unless you prefer not |

## Scope

In scope: `contracts/mandate-policy`, `contracts/approvals`, `scripts/deploy.mjs`, and the
invariants in [`docs/SPEC.md`](docs/SPEC.md) §2.

Out of scope: the OpenZeppelin `stellar-accounts` smart account and verifiers (report to
[OpenZeppelin](https://github.com/OpenZeppelin/stellar-contracts/security)); smart-account-kit;
the Stellar network itself; testnet resets; findings that need the owner's passkey or the
agent's secret key (holding the agent key lets you spend within the mandate, which is the
design).

## Threat model

The contracts add two things to a canonical OpenZeppelin smart account: a policy on the
agent's `:pay` rule (`fosay-mandate-policy`) and a queue for over-limit requests
(`fosay-approvals`). Each invariant below is checked by a test you can run with
`cargo test --workspace`.

### Invariants → tests

| # | Invariant (full text: SPEC §2) | Tests |
|---|---|---|
| I1 | The agent key only appears in rules scoped to the token and `fosay-approvals`; it can't change rules, unpause or edit its mandate | `mandate-policy::invariants::i1_agent_cannot_administer_its_mandate`, `enforce::agent_rule_cannot_pay_a_different_token` |
| I2 | No FOSAY contract moves funds on its own authority | `approvals::execute::owner_approves_over_limit_request`, `approvals::e2e::full_fosay_flow` |
| I3 | `enforce` requires a non-empty signer set, all of whom authenticated | `mandate-policy::invariants::i3_agent_rule_without_agent_signature_is_rejected` |
| I4 | No admin, no upgrade, no owner-settable config | No admin or upgrade entry points exist (review `contract.rs` of each crate) |
| I5 | The agent's secret key never reaches FOSAY servers | Off-chain; enforced in `fosay-app` (only the public key is registered) |
| I6 | An owner approval is bound to `execute(id)` and can't be replayed as a bare transfer | `approvals::execute::i6_owner_auth_is_rooted_at_execute`, `i6_approvals_are_not_interchangeable_with_bare_transfers` |
| I7 | `enforce` emits no events (x402 compatibility) | `mandate-policy::invariants::i7_agent_payment_emits_only_the_transfer_event` |

### Threats → controls

| Threat | Control | Tests |
|---|---|---|
| Agent overspends | Per-payment cap and rolling budget in `enforce`; fixed 9-slot ring, so a window can never exceed the budget | `enforce::payment_over_per_tx_cap_fails`, `rolling_budget_blocks_then_frees_after_window`, `budget::rolling_window_never_exceeds_budget` (property test) |
| Agent pays an unknown recipient | Recipient allowlist when set | `enforce::recipient_allowlist_is_enforced` |
| Agent calls something other than `transfer` | Context must be `transfer` on the mandate's token with 3 args | `enforce::non_transfer_calls_on_the_token_fail`, `zero_and_negative_amounts_fail` |
| Agent keeps spending after the owner says stop | Pause flag; revoking removes the rule | `enforce::paused_mandate_blocks_payments`, `admin::owner_revokes_agent_by_removing_the_rule` |
| Agent approves its own over-limit request | `execute` requires the wallet's auth; the agent's `:pay` rule still applies to the nested transfer | `execute::agent_cannot_execute_over_limit_but_can_within_limit` |
| Stranger fills the queue | `propose` requires the wallet's auth (agent `:ask` rule); per-account pending cap | `propose::stranger_cannot_propose_for_the_wallet`, `propose_caps_pending_per_account` |
| Replay or stale approval | Status machine (`Pending` only), ledger expiry | `execute::execute_rejects_expired_repeated_and_unknown`, `cancel_expire::expire_only_after_expiry_and_frees_slot` |
| Storage cost grows with use | Constant-size mandate entry | `enforce::mandate_entry_size_is_constant_across_payments` |

### Known limits

- `enforce` doesn't extend TTL (to keep x402 payments under facilitator fee ceilings). An
  off-chain keeper in `fosay-app` extends it. If the keeper stops for long enough, entries
  archive and must be restored before the agent can pay again; funds are not at risk.
- Budget windows are measured in ledgers, not wall-clock time.
- The policy trusts the canonical OZ account to call `enforce` only for contexts matched to
  its rule. FOSAY pins the account's WASM hash (`c514d35e…4b9e1`).
