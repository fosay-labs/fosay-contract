# FOSAY — Contract Specification (v0.1)

> **F**unds **O**nly **S**pent **A**fter **Y**es.
> A wallet you share with your AI agent. Your agent gets its own key with a
> spending limit you set. Small, in-scope payments go through on their own —
> the limit you set *is* the yes, given in advance. Anything bigger waits for
> you to approve it on your phone with your passkey. One tap revokes the agent.

Status: Phase 5 design. Not yet implemented. Every external dependency below was
verified against published source on 2026-11-09 (see "Verified facts").

---

## 1. What FOSAY builds vs. reuses

FOSAY deliberately writes as little security-critical code as possible.

| Piece | Source | FOSAY writes it? |
|---|---|---|
| Smart account (wallet) contract | OpenZeppelin `stellar-accounts` v0.8.0, canonical build deployed by `smart-account-kit` (WASM hash `c514d35e…4b9e1`) | **No** |
| WebAuthn (passkey) verifier | Canonical deployment (smart-account-kit) | **No** |
| Ed25519 verifier (agent key) | Canonical deployment (smart-account-kit) | **No** |
| Rolling spending-limit policy | OZ `spending_limit` v0.8.0, canonical deployment | **No** |
| Wallet deployment / factory | `smart-account-kit` `createWallet` | **No** |
| **Mandate policy** — per-tx cap, recipient allowlist, pause switch, mandate receipt | — | **Yes** (`fosay-mandate-policy`) |
| **Approvals queue** — over-limit requests the human approves with a passkey | — | **Yes** (`fosay-approvals`) |

Two contracts. Neither holds funds. Neither has an admin key.

---

## 2. Authorization model (how one wallet is configured)

One user wallet = one OZ smart account with these context rules:

| Rule | Context type | Signers | Policies | Purpose |
|---|---|---|---|---|
| `owner` (id 0, created by smart-account-kit) | `Default` | `External(webauthn_verifier, owner_passkey)` | none | Owner can do anything: approve, revoke, pause, change limits. **Must never be removed** (OZ caveat: losing the last rule covering the account locks it forever). |
| `<agent>:pay` | `CallContract(USDC_SAC)` | `External(ed25519_verifier, agent_pubkey)` | OZ `spending_limit` + `fosay-mandate-policy` | Agent pays within limits (x402, MPP charge, direct transfer). `valid_until` = mandate expiry. |
| `<agent>:ask` | `CallContract(fosay-approvals)` | same agent signer | none (⇒ OZ requires all signers to authenticate) | Agent can open/cancel approval requests. Nothing else. |

Rule names are capped at 20 bytes (`MAX_NAME_SIZE = 20`), e.g. `agt1:pay`, `agt1:ask`.

### Security invariants

- **I1.** The agent's key appears only in rules scoped to `USDC_SAC` and `fosay-approvals`. It can never authorize calls to the account itself, to the mandate policy, or to any other contract.
- **I2.** `fosay-approvals` never moves funds on its own authority. `execute` calls `token.transfer(account, …)`, which needs the account's authorization for *that exact transfer*. An over-limit transfer only passes under the `owner` rule (passkey).
- **I3.** Because a rule with policies defers signer checks to `enforce()` (OZ: "With policies, full signer validation is deferred to `enforce()`"), `fosay-mandate-policy::enforce` **must** assert that every signer in the rule is in `authenticated_signers`. OZ `spending_limit` only checks non-empty.
- **I4.** No upgrade or admin entry points in FOSAY contracts. New versions are new deployments.
- **I5.** The agent's secret key never reaches the FOSAY app or server. Only the public key is registered.

---

## 3. Dependency graph and deploy order

```
                 (external, already deployed)
   USDC SAC   OZ spending_limit v0.8.0   Ed25519 verifier   WebAuthn verifier
       ▲               ▲                        ▲                  ▲
       │               │                        │                  │
       │        installed on rule        signer identity     signer identity
       │               │                        │                  │
  ┌────┴───────────────┴────────────────────────┴──────────────────┴───┐
  │  per-user OZ smart account (canonical WASM c514d35e…, per-wallet)  │
  └──────┬───────────────────────────────────────────┬─────────────────┘
         │ install / enforce / uninstall             │ require_auth (propose/cancel)
         ▼                                           │ transfer auth (execute)
  fosay-mandate-policy                         fosay-approvals ──► USDC SAC.transfer
```

- `fosay-mandate-policy` and `fosay-approvals` do not call each other. Deploy in either order.
- Both must exist before an agent is connected to any wallet.
- Build: `soroban-sdk = "=28.0.0"`, `stellar-accounts = "=0.8.0"` (0.8.0 requires `soroban-sdk ^28`; do **not** use 29.x yet), target `wasm32v1-none`, Stellar CLI 28.x.

---

## 4. Contract: `fosay-mandate-policy`

Implements OZ `stellar_accounts::policies::Policy`. Multi-tenant: state keyed by
`(smart_account, context_rule_id)`. One deployment serves every FOSAY wallet.

### Storage

```rust
#[contracttype]
pub struct MandateParams {
    pub max_per_tx: i128,          // > 0, token base units (USDC: 7 decimals)
    pub recipients: Vec<Address>,  // empty = any recipient; max 20 entries
    pub mandate_hash: BytesN<32>,  // sha256 of the canonical mandate JSON the owner approved
}

#[contracttype]
pub struct MandateData {
    pub token: Address,            // copied from the rule's CallContract address at install
    pub max_per_tx: i128,
    pub recipients: Vec<Address>,
    pub mandate_hash: BytesN<32>,
    pub paused: bool,
    pub tx_count: u32,
}

#[contracttype]
pub enum DataKey {
    Mandate(Address, u32),         // (smart_account, context_rule_id) -> MandateData, persistent
}
```

TTL: extend on every read/write (threshold 29 days, extend to 30 days of ledgers, `DAY_IN_LEDGERS = 17280`, same as OZ).

### Functions

| Function | Auth | Behavior |
|---|---|---|
| `install(e, params: MandateParams, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Reject unless `context_rule.context_type == CallContract(token)` (`OnlyCallContractAllowed`). Reject if already installed, `max_per_tx <= 0`, or `recipients.len() > 20`. Store `MandateData { token, …, paused: false, tx_count: 0 }`. Emit `MandateInstalled`. |
| `enforce(e, context: Context, authenticated_signers: Vec<Signer>, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Load data (`NotInstalled`). If `paused` → `Paused`. **I3:** every `s` in `context_rule.signers` must be in `authenticated_signers` → else `SignerMissing`. `context` must be `Context::Contract` with `contract == token` and `fn_name == "transfer"` and 3 args → else `NotAllowed`. Decode `to = MuxedAddress::try_from_val(args[1]).address()`, `amount = i128::try_from_val(args[2])`. `amount <= 0` → `NotAllowed`. `amount > max_per_tx` → `PerTxLimitExceeded`. If `recipients` non-empty and `to ∉ recipients` → `RecipientNotAllowed`. `tx_count += 1`. Emit `MandateEnforced`. |
| `uninstall(e, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Remove entry. Emit `MandateUninstalled`. Called by OZ `remove_context_rule` / `remove_policy` (= **revoke agent**). |
| `set_paused(e, smart_account: Address, context_rule_id: u32, paused: bool)` | `smart_account.require_auth()` | Toggle. Emit `MandatePaused`. Agent can't call this (I1), so only the owner passkey can. (= **freeze / unfreeze agent**) |
| `update_mandate(e, smart_account: Address, context_rule_id: u32, params: MandateParams)` | `smart_account.require_auth()` | Same validation as install; keeps `token`, `paused`, `tx_count`. Emit `MandateUpdated`. (= **edit mandate**) |
| `get_mandate(e, smart_account: Address, context_rule_id: u32) -> MandateData` | none | View. |

### Events (`#[contractevent]`)

| Event | Topics | Data |
|---|---|---|
| `MandateInstalled` | `smart_account`, `context_rule_id` | `token, max_per_tx, recipients, mandate_hash` |
| `MandateEnforced` | `smart_account`, `context_rule_id` | `to: Address, amount: i128, mandate_hash, tx_count` |
| `MandateUpdated` | `smart_account`, `context_rule_id` | `max_per_tx, recipients, mandate_hash` |
| `MandatePaused` | `smart_account`, `context_rule_id` | `paused: bool` |
| `MandateUninstalled` | `smart_account`, `context_rule_id` | — |

`MandateEnforced` + OZ `SpendingLimitEnforced` are the agent-payment receipt: every
autonomous payment is linked to the `mandate_hash` the owner approved.

### Errors (`#[contracterror]`, range 4100–4199)

`NotInstalled = 4100, AlreadyInstalled = 4101, OnlyCallContractAllowed = 4102,
InvalidParams = 4103, Paused = 4104, SignerMissing = 4105, NotAllowed = 4106,
PerTxLimitExceeded = 4107, RecipientNotAllowed = 4108`

Note: when `__check_auth` fails, callers only see `Error(Auth, InvalidAction)` —
policy error codes surface in simulation diagnostics, not to the end caller. The
SDK must read simulation diagnostics to tell the agent *why* (e.g. "over limit →
open an approval request").

---

## 5. Contract: `fosay-approvals`

A queue of over-limit payment requests. Holds no funds, has no admin.

### Storage

```rust
#[contracttype]
pub enum IntentStatus { Pending, Executed, Cancelled, Expired }

#[contracttype]
pub struct Intent {
    pub id: u64,
    pub account: Address,          // the FOSAY smart account paying
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    pub mandate_hash: BytesN<32>,  // which mandate/task this belongs to (may be zero bytes)
    pub memo: String,              // agent's human-readable reason, max 140 bytes
    pub created_ledger: u32,
    pub expires_ledger: u32,
    pub status: IntentStatus,
}

#[contracttype]
pub struct Config { pub max_pending_per_account: u32, pub max_ttl_ledgers: u32 }

#[contracttype]
pub enum DataKey {
    Config,                        // instance
    NextId,                        // instance, u64
    Intent(u64),                   // persistent
    PendingCount(Address),         // persistent, u32
}
```

### Functions

| Function | Auth | Behavior |
|---|---|---|
| `__constructor(e, max_pending_per_account: u32, max_ttl_ledgers: u32)` | deploy-time | Store `Config`. Suggested: 20 pending, 120 960 ledgers (~7 days). Immutable afterwards. |
| `propose(e, account: Address, token: Address, to: Address, amount: i128, mandate_hash: BytesN<32>, memo: String, ttl_ledgers: u32) -> u64` | `account.require_auth()` — satisfied by the agent's `:ask` rule or the owner rule | `amount > 0`, `0 < ttl_ledgers <= max_ttl_ledgers`, `memo.len() <= 140`, `PendingCount(account) < max_pending` → else errors. Store `Intent{status: Pending}`, increment `PendingCount`, `NextId`. Emit `IntentProposed`. Return id. (= **agent asks for a yes**) |
| `execute(e, id: u64)` | none at this level (anyone may submit, e.g. a relayer); funds move only if `account` authorizes `token.transfer(account, to, amount)` — in practice the owner passkey under the `owner` rule | Status must be `Pending` (`NotPending`); `ledger <= expires_ledger` (`Expired`). Set `Executed`, decrement `PendingCount`, then `token::TokenClient::new(&token).transfer(&account, &to, &amount)`. Emit `IntentExecuted`. Atomic: if transfer auth fails, nothing changes. (= **human says yes**) |
| `cancel(e, id: u64)` | `intent.account.require_auth()` | `Pending` → `Cancelled`, decrement count. Emit `IntentCancelled`. Owner uses it to **say no**; agent may use it to withdraw its own request. |
| `expire(e, id: u64)` | none | If `Pending` and `ledger > expires_ledger` → `Expired`, decrement count. Emit `IntentExpired`. Frees pending slots; callable by the indexer/cron. |
| `get_intent(e, id: u64) -> Intent` | none | View. |
| `pending_count(e, account: Address) -> u32` | none | View. |

Lists ("show my pending requests") come from the indexer reading events, not from
on-chain iteration.

### Events

| Event | Topics | Data |
|---|---|---|
| `IntentProposed` | `account`, `id` | `token, to, amount, mandate_hash, memo, expires_ledger` |
| `IntentExecuted` | `account`, `id` | `to, amount` |
| `IntentCancelled` | `account`, `id` | — |
| `IntentExpired` | `account`, `id` | — |

`IntentProposed` is what triggers the phone push notification.

### Errors (range 4200–4299)

`NotFound = 4200, NotPending = 4201, Expired = 4202, NotYetExpired = 4203,
InvalidAmount = 4204, InvalidTtl = 4205, MemoTooLong = 4206, TooManyPending = 4207`

---

## 6. Every function mapped to a product step

| Product step (user flow) | On-chain calls |
|---|---|
| 1. Create wallet with passkey | smart-account-kit `createWallet` (canonical OZ account, `owner` rule) — no FOSAY contract |
| 2. Connect an agent (agent generates its own ed25519 key, sends pubkey) | owner passkey signs: OZ `add_context_rule(CallContract(USDC), "agt1:pay", valid_until, [agent signer], {spending_limit: {limit, period}, mandate_policy: MandateParams})` → triggers `spending_limit.install` + `fosay-mandate-policy.install`; then `add_context_rule(CallContract(approvals), "agt1:ask", valid_until, [agent signer], {})` |
| 3. Agent pays within limits (x402 / MPP charge / transfer) | USDC `transfer` → account `__check_auth` → `spending_limit.enforce` + `fosay-mandate-policy.enforce` |
| 4. Agent hits a limit, asks for approval | `fosay-approvals.propose` (agent `:ask` rule) |
| 5. Human approves on phone | `fosay-approvals.execute` with owner passkey authorizing the inner `transfer` |
| 6. Human rejects | `fosay-approvals.cancel` (owner) |
| 7. Freeze / unfreeze agent | `fosay-mandate-policy.set_paused` (owner) |
| 8. Edit limits / allowlist | OZ spending_limit setter + `fosay-mandate-policy.update_mandate` (owner) |
| 9. Revoke agent (one tap) | OZ `remove_context_rule` ×2 (owner) → `uninstall` on both policies |
| 10. Mandate expires | OZ rule `valid_until` passes → agent rules rejected automatically |
| 11. Activity feed / receipts | Indexer reads `MandateEnforced`, `SpendingLimitEnforced`, `Intent*` events |

No function exists that isn't on this list.

---

## 7. Phase-5 gate: spikes to run before writing contract code

These are the unverified assumptions. Each is a short testnet script. If one fails,
the design changes before code is written.

| # | Assumption | Evidence so far | How to test |
|---|---|---|---|
| S1 | The x402 facilitator accepts a smart account (C-address) as payer | `@x402/stellar@2.30.0` facilitator: no C-address rejection; "signed" = signature not `scvVoid`; rejects auth sub-invocations (none here with External signers). **But** the stock client (`ExactStellarScheme` + `createEd25519Signer`) signs as a classic G-account and cannot produce an OZ `AuthPayload`. | Build the payment tx manually: USDC `transfer(C-account, payTo, amount)`, sign with agent ed25519 via the OZ digest (`auth_digest`), submit to the testnet facilitator's `/verify` and `/settle`. If it passes, FOSAY ships its own x402 client signer in `fosay-app`. |
| S2 | `fosay-approvals.execute` works when the owner authorizes only the inner `token.transfer` (non-root auth entry) | Standard Soroban behavior, but not yet exercised with an OZ account + WebAuthn | Testnet: propose → execute with passkey-signed auth entry rooted at `USDC.transfer`. |
| S3 | Two `add_context_rule` calls can be done with one passkey prompt | Unknown for the canonical account build | Check whether the canonical account exposes a batch/execute entry point; otherwise accept two prompts in v1. |
| S4 | Facilitator fee ceiling covers smart-account `__check_auth` (verifier + 2 policies) | Facilitator uses `maxTransactionFeeStroops` as a ceiling | Measure resource fee of an S1 payment. |

---

## 8. Verified facts (2026-11-09)

- `stellar-accounts` latest = **0.8.0**, depends on `soroban-sdk ^28.0.0`. `soroban-sdk` latest stable = 29.0.1 (not compatible with stellar-accounts 0.8.0).
- OZ limits: 15 signers/rule, 5 policies/rule, rule name ≤ 20 bytes, unlimited rules. Caller supplies one rule id per auth context in `AuthPayload::context_rule_ids`.
- OZ `spending_limit` (v0.8.0): only intercepts `transfer`, amount = `args[2]`; requires `CallContract` rule; rolling window in ledgers; `MAX_HISTORY_ENTRIES = 815`; error `SpendingLimitExceeded = 3221`.
- SAC `transfer` signature in soroban-sdk 28: `transfer(from: Address, to: MuxedAddress, amount: i128)`; `MuxedAddress::address()` exists.
- smart-account-kit **0.10.0** (2026-10-27): not independently audited (its own README). New wallets should use account WASM `c514d35ea67eaea9bfc1fb3327854c0c06111bc1ddebd4fe217c504c1004b9e1`.
- Testnet: WebAuthn verifier `CC7EKIHQP3TN4CARQDND6CEOY2UXLWWC2X5GHTD5NLAT7BG5GPZIOM3F`, Ed25519 verifier `CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4`, spending-limit v0.8.0 `CC3CEYQAP6NPNFIYSWL5MVZRCPJZGWX4XO7WDEJOUVGO2FZX5OONYPA5`.
- Mainnet: WebAuthn verifier `CB7HENHJ7NF34I5FFXQK7D5I3WWQRGB5O5XO77D3NXMT7LM7LOKRQ5YR`, Ed25519 verifier `CBOOZV2BK5OETGL4Q4KGEBESPRLJFN7DOFWDT7OZGLD7EQEZUVOWUEMC`, spending-limit v0.8.0 `CDB2CVES2QMAL73US4EEAM43B4V3MBEP4K6I2UM7XZ25O4GGQE5CY3UG`.
- `@x402/stellar` latest = **2.30.0** (2026-11-04); facilitators always sponsor fees; default payment expiry ~12 ledgers.

Sources: crates.io (`stellar-accounts` 0.8.0 source, `soroban-sdk` 28.0.0 source),
npm (`@x402/stellar` 2.30.0, `smart-account-kit` 0.10.0),
`github.com/stellar/smart-account-kit/docs/deployments-*.md`.
Re-verify contract IDs before mainnet.
