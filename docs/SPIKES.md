# FOSAY — Phase-5 Spike Results (testnet, 2026-11-09)

These spikes tested the unverified assumptions in [`SPEC.md` §7](./SPEC.md#7-phase-5-gate-spikes-to-run-before-writing-contract-code)
on Stellar **testnet** (protocol 30), using the **canonical** OpenZeppelin
`stellar-accounts` v0.8.0 smart-account WASM (`c514d35e…4b9e1`), the canonical
Ed25519 verifier and the canonical v0.8.0 spending-limit policy, all deployed by
smart-account-kit. Scripts live in `spikes/` (Node 22, `@stellar/stellar-sdk@16.4.1`,
`@x402/stellar@2.30.0`).

Test fixtures: a self-issued classic asset `FUSD` with its SAC, standing in for USDC,
since testnet USDC comes from a browser faucet. The x402 checks don't depend on the
asset, as long as it is a SEP-41 token.

| Spike | Question | Result |
|---|---|---|
| Setup | Can we sign for an OZ v0.8.0 smart account ourselves? | **Yes.** Owner-signed `add_context_rule` succeeded on-chain with our `AuthPayload` / preimage-v1 digest implementation. |
| S1 | Does the x402 facilitator accept a smart account as payer? | **Yes, with two conditions** (below). Settled tx `fcfc52c2…4466`. |
| S2 | Can `approvals.execute` move funds when the owner authorizes only the inner `transfer`? | **Yes, but don't design it that way.** Use the rooted variant (S2b): `execute` calls `account.require_auth()` first. Owner-approved 7 FUSD over a 5 FUSD agent limit; fee 12,284. |
| S3 | Can connecting an agent be one passkey prompt? | **Not with the canonical account.** It has no multi-call entry point for rules; `execute` takes a single target. v1 = two prompts. |
| S4 | Does the facilitator's fee ceiling cover smart-account auth? | **Yes in steady state without policies, no with OZ spending-limit, no on the first payment after a TTL lapse.** |

## S1 — x402 with a smart-account payer

The official `ExactStellarScheme` facilitator (run locally with our own facilitator
key, so the exact same verify/settle code) verified and settled a 1 FUSD payment
from smart account `CB22TWCJ…DTR7`, authorized **only by the agent's Ed25519 key**
under a `CallContract(FUSD)` rule.

**Condition 1: no policy may emit an event.** The facilitator's
`validateSimulationEvents` rejects the payment if *any* contract event in the
simulation is not the token `transfer` (`invalid_exact_stellar_payload_event_not_transfer`).
The OZ spending-limit policy emits `spending_limit_enforced` on every enforce, so
**any agent rule using OZ `spending_limit` is rejected by stock x402**. The same
payment under a rule with no policies passed.

**Condition 2: the stock client can't do it.** `@x402/stellar`'s client signs as a
classic G-account. FOSAY must ship its own x402 client signer that builds the OZ
`AuthPayload`. That's done in the spike (`signSmartAccountEntry` in `spikes/common.mjs`,
about 40 lines).

The facilitator also enforces: payment auth expiry ≤ current ledger + ~12 (+2
tolerance), no sub-invocations in the auth entry, and exactly one transfer event.

Negative checks (both rejected at simulation with `Error(Auth, InvalidAction)`):
- agent tries 6 FUSD when the rolling limit is 5 FUSD
- owner's key presented under the agent's rule

## S2 — the approval queue's `execute` pattern

Spike contract `spikes/s2-contract` (built with Stellar CLI 30.0.1, soroban-sdk 28.0.0).

**S2a, non-root (spec v0.1 design): works, but has a flaw.** `pay_from` calls
`token.transfer(account, …)` without requiring auth itself. The owner's signed entry
is rooted at `SAC.transfer` (`4ec23bac…f1b1`, fee 11,684). It needs
`simulateTransaction(..., authMode: "record_allow_nonroot")`; default recording
simulation fails with `Error(Auth, InvalidAction)`. **Flaw:** that signed entry
authorizes a bare `transfer(account, to, amount)`, so anyone holding it can submit it
straight to the token, bypassing the queue. Funds still go to the approved recipient,
but the intent stays `Pending` and the on-chain record becomes wrong.

**S2b, rooted (adopt this): works cleanly.** `pay_rooted` calls `from.require_auth()`
first, then `transfer`. The owner signs **one** entry rooted at the queue call, with
`transfer` as its sub-invocation (`b3469845…5871`, fee 12,284). Default simulation
mode works. Two auth contexts mean `context_rule_ids = [owner, owner]`; giving only one
id is rejected.

Safety under agent rules: the root context (`CallContract(approvals)`) matches the
agent's `:ask` rule, but the nested `transfer` must still pass the agent's `:pay` rule
and its limits. In S2a the agent was blocked at 7 FUSD and allowed at 0.5 FUSD (within
its own limit, so harmless). Only the owner can push an over-limit intent through.

## S4 — fees (stroops; facilitator default ceiling = 50,000)

| Payer | 1st payment | Steady state (fee charged) | Passes default ceiling? |
|---|---|---|---|
| Plain G-account, stock x402 client | — | **10,887** | yes |
| Smart account, agent rule, no policies | ~202,000 (TTL renewal) | **11,332** (min resource fee 16,321) | yes, steady state |
| Smart account, agent rule + OZ spending-limit | ~320,000 | **~48,300** (min resource fee ~58,500) | **no** |

- In steady state the smart-account signature check costs about the same as a classic account.
- The expensive first payment is TTL extension (OZ extends entries to 30 days once
  they fall under 29 days left). This repeats roughly daily, so a hosted facilitator
  with a 50k ceiling would reject the first payment of each day.
- OZ spending-limit writes a growing `spending_history` vector on every payment,
  which puts it above the default ceiling.

## Design consequences (to fold into SPEC.md v0.2)

1. **Replace OZ `spending_limit` on the agent `:pay` rule with one lean FOSAY policy**
   (mandate + rolling budget combined) that:
   - emits **no events in `enforce`** (events only on install/update/pause/uninstall),
   - keeps **constant-size** budget state (fixed buckets, not a growing history),
   - targets a steady-state fee under 50,000 stroops; to be measured.

   Trade-off: more custom security-critical code instead of OZ's audited policy. It
   still implements the OZ `Policy` trait and runs inside the audited account.
2. **Receipts come from the indexer, not from policy events.** It reads the tx
   envelope's `AuthPayload.context_rule_ids` plus the SAC `transfer` event to attribute
   each payment to an agent rule.
3. **TTL keeper.** `fosay-app` runs a job that extends the TTL of each wallet's
   account, rule and policy entries before they drop under the 29-day threshold, so
   x402 payments stay in the cheap steady state.
4. **FOSAY x402 client signer** goes in `fosay-app/packages/sdk`.
5. **`fosay-approvals.execute` must call `intent.account.require_auth()`** before
   the transfer (S2b), so the owner's approval is bound to `execute(id)` and can't be
   replayed as a bare transfer. Clients pass two rule ids (`[owner, owner]`).
6. **Upstream:** propose to `x402-foundation/x402` that the Stellar facilitator only
   validate events emitted by the asset contract, so smart accounts with
   event-emitting policies can pay. (Not filed yet; needs maintainer approval.)
7. **Note:** the canonical account exposes `upgrade(new_wasm_hash, operator)`. Agent
   rules never cover `CallContract(account)` (invariant I1), so only the owner can
   upgrade.
