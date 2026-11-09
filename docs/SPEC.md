# FOSAY — Contract Specification (v0.2)

> **F**unds **O**nly **S**pent **A**fter **Y**es.
> A wallet you share with your AI agent. Your agent gets its own key with a
> spending limit you set. Small, in-scope payments go through on their own:
> the limit you set *is* the yes, given in advance. Anything bigger waits for you
> to approve it on your phone with your passkey. One tap revokes the agent.

**Status:** design validated on testnet (see [`SPIKES.md`](./SPIKES.md)); implemented in `contracts/`, deployed to testnet (see `docs/DEPLOYMENT.md`).

**Changes from v0.1** (all driven by the testnet spikes):
1. The agent's `:pay` rule uses **one** FOSAY policy that also enforces the rolling
   budget. OZ `spending_limit` is dropped: it emits an event on every payment, which
   the x402 facilitator rejects, and it costs over the facilitator's fee ceiling (S1, S4).
2. `fosay-mandate-policy::enforce` emits **no events** and keeps **constant-size** state.
3. `fosay-approvals::execute` calls `intent.account.require_auth()` **before** the
   transfer, so the owner's approval is bound to `execute(id)` and can't be replayed
   as a bare token transfer (S2b).
4. Payment receipts come from the indexer (tx envelope + transfer event), not from
   policy events.
5. A TTL keeper (in `fosay-app`) keeps wallet entries in the cheap steady state.
6. Connecting an agent is two owner signatures (S3).

---

## 1. What FOSAY builds vs. reuses

| Piece | Source | FOSAY writes it? |
|---|---|---|
| Smart account (wallet) contract | OpenZeppelin `stellar-accounts` v0.8.0, canonical build deployed by `smart-account-kit` (WASM hash `c514d35e…4b9e1`) | **No** |
| WebAuthn (passkey) verifier | Canonical deployment | **No** |
| Ed25519 verifier (agent key) | Canonical deployment | **No** |
| Wallet deployment | `smart-account-kit` `createWallet` | **No** |
| **Mandate policy**: per-tx cap, rolling budget, recipient allowlist, pause, mandate hash | — | **Yes** (`fosay-mandate-policy`) |
| **Approvals queue**: over-limit requests the human approves with a passkey | — | **Yes** (`fosay-approvals`) |

Two contracts. Neither holds funds. Neither has an admin key or an upgrade entry point.

---

## 2. Authorization model (one wallet)

| Rule | Context type | Signers | Policies | Purpose |
|---|---|---|---|---|
| `owner` (id 0, created by the wallet constructor) | `Default` | `External(webauthn_verifier, owner_passkey)` | none | Owner can do anything: approve, revoke, pause, change limits, upgrade the account. **Must never be removed** (OZ: losing the last rule that covers the account locks it permanently). |
| `<agent>:pay` | `CallContract(USDC_SAC)` | `External(ed25519_verifier, agent_pubkey)` | `fosay-mandate-policy` only | Agent pays within the mandate (x402, MPP charge, direct transfer). `valid_until` = mandate expiry. |
| `<agent>:ask` | `CallContract(fosay-approvals)` | same agent signer | none (OZ then requires all signers to authenticate) | Agent can open, cancel, or execute requests. Execute still needs the nested transfer to pass `:pay`. |

Rule names are at most 20 bytes (`MAX_NAME_SIZE = 20`), e.g. `agt1:pay`, `agt1:ask`.
In every authorization the client supplies one rule id per auth context
(`AuthPayload.context_rule_ids`), in auth-context order.

### Security invariants

- **I1.** The agent key appears only in rules scoped to `USDC_SAC` and `fosay-approvals`.
  It never covers `CallContract(account)`, `CallContract(fosay-mandate-policy)`, or
  `Default`. So it can't change rules, unpause itself, edit its mandate, or upgrade the account.
- **I2.** No FOSAY contract moves funds on its own authority. Every token movement is a
  `transfer(account, …)` that the account authorizes for that exact call.
- **I3.** A rule with policies defers signer checks to the policies (OZ: "with policies,
  full signer validation is deferred to `enforce()`"). `fosay-mandate-policy::enforce`
  **must** assert that the rule has at least one signer and that every signer of the
  rule is in `authenticated_signers`. It is the only policy on `:pay`, so nothing else
  checks this. The non-empty check means a misconfigured rule with no signers can never
  spend.
- **I4.** No admin, no upgrade, no owner-settable config in FOSAY contracts. New
  versions are new deployments.
- **I5.** The agent's secret key never reaches FOSAY's app or servers. Only its public
  key is registered.
- **I6.** An owner approval of a request is bound to `fosay-approvals.execute(id)`. The
  signed auth entry's root invocation is `execute(id)`, with the `transfer` as its only
  sub-invocation.
- **I7.** `fosay-mandate-policy::enforce` emits no events, so payments under `:pay` stay
  compatible with the x402 Stellar facilitator, which rejects any non-transfer event.

---

## 3. Dependency graph and build order

```
          (external, already deployed / canonical)
   USDC SAC     Ed25519 verifier     WebAuthn verifier
       ▲               ▲                    ▲
       │        signer identity      signer identity
  ┌────┴───────────────┴────────────────────┴──────────────────────┐
  │ per-user OZ smart account (canonical WASM c514d35e…, per wallet)│
  └──────┬────────────────────────────────────────┬────────────────┘
         │ install / enforce / uninstall           │ account.require_auth()
         ▼                                         ▼   (propose / cancel / execute)
  fosay-mandate-policy                      fosay-approvals ──► USDC SAC.transfer
```

- The two FOSAY contracts don't call each other. Build, test and deploy them in either order.
- Both must be deployed before any agent is connected.
- Toolchain (verified 2026-11-09): Rust stable (1.97 tested), `soroban-sdk = "=28.0.0"`,
  `stellar-accounts = "=0.8.0"` (needs `soroban-sdk ^28`; don't move to 29.x until
  `stellar-accounts` supports it), target `wasm32v1-none`. **Wasm builds must go through
  `stellar contract build`** (Stellar CLI ≥ 25.2.0; 30.0.1 tested): soroban-sdk 28 refuses
  a plain `cargo build --target wasm32v1-none`. Native `cargo test` works directly.

---

## 4. Contract: `fosay-mandate-policy`

Implements OZ `stellar_accounts::policies::Policy`. Multi-tenant: state is keyed by
`(smart_account, context_rule_id)`. One deployment serves every FOSAY wallet.

### Constants

```rust
pub const BUCKETS: u32 = 8;                 // budget-window granularity
pub const MAX_RECIPIENTS: u32 = 20;
pub const MIN_PERIOD_LEDGERS: u32 = BUCKETS; // at least one ledger per bucket
pub const MAX_PERIOD_LEDGERS: u32 = 535_680; // 31 days
pub const DAY_IN_LEDGERS: u32 = 17_280;
pub const TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;
pub const TTL_THRESHOLD: u32 = TTL_EXTEND_TO - DAY_IN_LEDGERS;
```

### Storage

```rust
#[contracttype]
pub struct MandateParams {
    pub budget: i128,              // max total spend in any rolling window of period_ledgers
    pub period_ledgers: u32,       // MIN_PERIOD_LEDGERS..=MAX_PERIOD_LEDGERS
    pub max_per_tx: i128,          // 0 < max_per_tx <= budget
    pub recipients: Vec<Address>,  // empty = any recipient; at most MAX_RECIPIENTS
    pub mandate_hash: BytesN<32>,  // sha256 of the canonical mandate JSON the owner approved
}

#[contracttype]
pub struct Bucket { pub index: u32, pub amount: i128 }

#[contracttype]
pub struct MandateData {
    pub token: Address,            // the rule's CallContract address, fixed at install
    pub budget: i128,
    pub period_ledgers: u32,       // fixed at install (changing it needs a new rule)
    pub max_per_tx: i128,
    pub recipients: Vec<Address>,
    pub mandate_hash: BytesN<32>,
    pub paused: bool,
    pub buckets: Vec<Bucket>,      // ring of exactly BUCKETS + 1 slots, created at install
}

#[contracttype]
pub enum DataKey {
    Mandate(Address, u32),         // (smart_account, context_rule_id) -> MandateData, persistent
}
```

### Rolling budget (fixed-size ring, never over budget)

- `bucket_len = ceil(period_ledgers / BUCKETS)`; `idx = ledger.sequence() / bucket_len`.
- Spend lives in a ring of exactly `SLOTS = BUCKETS + 1` slots `{ index, amount }`,
  created zeroed at install. Bucket `idx` lives in slot `idx % SLOTS`; writing to a
  slot that holds an older index overwrites it.
- The live window is buckets `idx - BUCKETS ..= idx` (saturating): `SLOTS` consecutive
  indices, so they occupy distinct slots. Stale slots are ignored when summing.
- Any `period_ledgers`-long interval ending now falls inside the live window, so **total
  spend in any rolling window of `period_ledgers` never exceeds `budget`**. The trade-off:
  the window can be up to one `bucket_len` stricter than the nominal period, never looser.
- **The ring never changes length, so the entry's byte size is constant.** A payment
  never pays rent for growing storage. Phase 8 testnet measurement: a growing list made
  the first payment cost ~54k stroops vs ~19k in steady state, over the x402 ceiling.
- All sums use `checked_add`; overflow → `MathOverflow`.

### Functions

| Function | Auth | Behavior |
|---|---|---|
| `install(e, params: MandateParams, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Reject unless `context_rule.context_type` is `CallContract(token)` (`OnlyCallContractAllowed`). Reject if already installed (`AlreadyInstalled`). Validate params (`InvalidParams`): `budget > 0`, `0 < max_per_tx <= budget`, period in range, `recipients.len() <= MAX_RECIPIENTS`. Store `MandateData { token, …, paused: false, buckets: <9 zeroed slots> }`, extend TTL. Emit `MandateInstalled`. |
| `enforce(e, context: Context, authenticated_signers: Vec<Signer>, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Load (`NotInstalled`). `paused` → `Paused`. **I3:** `context_rule.signers` must be non-empty and every signer in it must be in `authenticated_signers`, else `SignerMissing`. `context` must be `Context::Contract` with `contract == token`, `fn_name == "transfer"`, 3 args, else `NotAllowed`. `to = MuxedAddress::try_from_val(args[1])?.address()`, `amount = i128::try_from_val(args[2])?` (decode failure → `NotAllowed`). `amount <= 0` → `NotAllowed`. `amount > max_per_tx` → `PerTxLimitExceeded`. Recipients non-empty and `to ∉ recipients` → `RecipientNotAllowed`. `spent(window) + amount > budget` → `BudgetExceeded`. Add `amount` to the current bucket's slot (overwriting a stale slot) and write. **No event (I7). No TTL extension**, which keeps x402 payments cheap; the keeper extends TTL. |
| `uninstall(e, context_rule: ContextRule, smart_account: Address)` | `smart_account.require_auth()` | Remove the entry; no error if missing. Emit `MandateUninstalled`. Called by OZ `remove_context_rule` / `remove_policy` (= **revoke agent**). |
| `set_paused(e, smart_account: Address, context_rule_id: u32, paused: bool)` | `smart_account.require_auth()` | Set flag, extend TTL. Emit `MandatePaused`. Only the owner can call this (I1). (= **freeze / unfreeze**) |
| `update_mandate(e, smart_account: Address, context_rule_id: u32, budget: i128, max_per_tx: i128, recipients: Vec<Address>, mandate_hash: BytesN<32>)` | `smart_account.require_auth()` | Same validation as install. Keeps `token`, `period_ledgers`, `paused`, `buckets` (spend so far still counts). Extend TTL. Emit `MandateUpdated`. (= **edit mandate**) |
| `extend_ttl(e, smart_account: Address, context_rule_id: u32)` | none | Extend the entry's TTL (`TTL_THRESHOLD`, `TTL_EXTEND_TO`). `NotInstalled` if missing. Called by the keeper. |
| `get_mandate(e, smart_account: Address, context_rule_id: u32) -> MandateData` | none | View. |
| `remaining_budget(e, smart_account: Address, context_rule_id: u32) -> i128` | none | `budget - spent_in_window` (0 if paused). The agent calls this to decide between paying and proposing. |

### Events (`#[contractevent]`; never emitted from `enforce`)

| Event | Topics | Data |
|---|---|---|
| `MandateInstalled` | `smart_account`, `context_rule_id` | `token, budget, period_ledgers, max_per_tx, recipients, mandate_hash` |
| `MandateUpdated` | `smart_account`, `context_rule_id` | `budget, max_per_tx, recipients, mandate_hash` |
| `MandatePaused` | `smart_account`, `context_rule_id` | `paused` |
| `MandateUninstalled` | `smart_account`, `context_rule_id` | — |

### Errors (`#[contracterror]`, 4100–4199)

`NotInstalled = 4100, AlreadyInstalled = 4101, OnlyCallContractAllowed = 4102,
InvalidParams = 4103, Paused = 4104, SignerMissing = 4105, NotAllowed = 4106,
PerTxLimitExceeded = 4107, RecipientNotAllowed = 4108, BudgetExceeded = 4109,
MathOverflow = 4110`

When a policy panics inside `__check_auth`, the caller only sees
`Error(Auth, InvalidAction)`. The specific code shows up in simulation diagnostics,
so the SDK must read diagnostics to tell the agent *why* (e.g. `BudgetExceeded` →
open an approval request).

### Acceptance criteria (measured in Phase 8 on testnet)

- Steady-state x402 payment under `:pay` has a min resource fee **< 50,000 stroops**
  (the facilitator's default ceiling).
- The official x402 facilitator verifies and settles it (no extra events).

---

## 5. Contract: `fosay-approvals`

A queue of payment requests over the agent's limits. Holds no funds, has no admin.

### Storage

```rust
#[contracttype]
pub enum IntentStatus { Pending, Executed, Cancelled, Expired }

#[contracttype]
pub struct Intent {
    pub id: u64,
    pub account: Address,          // the FOSAY smart account that pays
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    pub mandate_hash: BytesN<32>,  // task/mandate this belongs to (may be all zeroes)
    pub memo: String,              // agent's reason, at most 140 bytes
    pub created_ledger: u32,
    pub expires_ledger: u32,
    pub status: IntentStatus,
}

#[contracttype]
pub struct Config { pub max_pending_per_account: u32, pub max_ttl_ledgers: u32 }

#[contracttype]
pub enum DataKey {
    Config,                        // instance
    NextId,                        // instance, u64, starts at 1
    Intent(u64),                   // persistent
    PendingCount(Address),         // persistent, u32
}
```

### Functions

| Function | Auth | Behavior |
|---|---|---|
| `__constructor(e, max_pending_per_account: u32, max_ttl_ledgers: u32)` | deploy | Store `Config` (suggested 20 and 120,960 ≈ 7 days). Both must be > 0. Immutable. |
| `propose(e, account: Address, token: Address, to: Address, amount: i128, mandate_hash: BytesN<32>, memo: String, ttl_ledgers: u32) -> u64` | `account.require_auth()` (agent `:ask` rule or owner) | `amount > 0` (`InvalidAmount`), `0 < ttl_ledgers <= max_ttl_ledgers` (`InvalidTtl`), `memo.len() <= 140` (`MemoTooLong`), `PendingCount(account) < max_pending` (`TooManyPending`). Store `Intent { status: Pending }`, increment `PendingCount` and `NextId`, extend TTLs. Emit `IntentProposed`. Return id. (= **agent asks for a yes**) |
| `execute(e, id: u64)` | `intent.account.require_auth()` **first** (I6), then the nested `transfer` needs the account's auth too | Load (`NotFound`); `Pending` (`NotPending`); `ledger <= expires_ledger` (`Expired`). `account.require_auth()`. Set `Executed`, decrement `PendingCount`, then `token::TokenClient::new(&e, &token).transfer(&account, &to, &amount)`. Emit `IntentExecuted`. Atomic. The owner signs one auth entry (root `execute(id)`, sub-invocation `transfer`) with `context_rule_ids = [owner, owner]`. The agent can only execute requests whose transfer also passes its `:pay` rule. (= **human says yes**) |
| `cancel(e, id: u64)` | `intent.account.require_auth()` | `Pending` → `Cancelled`, decrement count. Emit `IntentCancelled`. Owner = **say no**; agent = withdraw. |
| `expire(e, id: u64)` | none | `Pending` and `ledger > expires_ledger` → `Expired`, decrement count; otherwise `NotYetExpired` / `NotPending`. Emit `IntentExpired`. Frees slots; called by the keeper. |
| `get_intent(e, id: u64) -> Intent` | none | View. |
| `pending_count(e, account: Address) -> u32` | none | View. |

TTL: `Intent(id)` and `PendingCount` extended on every write (`TTL_THRESHOLD`/`TTL_EXTEND_TO`
as above). Instance storage extended on every call. Lists come from the indexer.

### Events

| Event | Topics | Data |
|---|---|---|
| `IntentProposed` | `account`, `id` | `token, to, amount, mandate_hash, memo, expires_ledger` |
| `IntentExecuted` | `account`, `id` | `to, amount` |
| `IntentCancelled` | `account`, `id` | — |
| `IntentExpired` | `account`, `id` | — |

`IntentProposed` triggers the phone push notification. **Client rule:** the approval
screen must show the amount and recipient decoded from the auth entry's
`transfer` sub-invocation that the passkey will sign, not just indexer data.

### Errors (4200–4299)

`NotFound = 4200, NotPending = 4201, Expired = 4202, NotYetExpired = 4203,
InvalidAmount = 4204, InvalidTtl = 4205, MemoTooLong = 4206, TooManyPending = 4207,
InvalidConfig = 4208`

---

## 6. Every function mapped to a product step

| Product step | On-chain calls |
|---|---|
| 1. Create wallet with passkey | smart-account-kit `createWallet` (canonical OZ account; `owner` rule 0) |
| 2. Connect an agent (agent generates its own Ed25519 key, sends only the public key) | Owner signature 1: OZ `add_context_rule(CallContract(USDC), "agt1:pay", valid_until, [agent], {mandate_policy: MandateParams})` → `fosay-mandate-policy.install`. Owner signature 2: `add_context_rule(CallContract(approvals), "agt1:ask", valid_until, [agent], {})` |
| 3. Agent checks headroom | `fosay-mandate-policy.remaining_budget` |
| 4. Agent pays within its mandate (x402 / MPP charge / transfer) | USDC `transfer` → account `__check_auth` → `fosay-mandate-policy.enforce` |
| 5. Agent hits a limit and asks | `fosay-approvals.propose` (agent `:ask` rule) |
| 6. Human approves on phone | `fosay-approvals.execute(id)`, owner passkey, rule ids `[0, 0]` |
| 7. Human rejects | `fosay-approvals.cancel` (owner) |
| 8. Freeze / unfreeze agent | `fosay-mandate-policy.set_paused` (owner) |
| 9. Edit budget / caps / allowlist | `fosay-mandate-policy.update_mandate` (owner) |
| 10. Revoke agent (one action per rule) | OZ `remove_context_rule` for `:pay` and `:ask` → `uninstall` |
| 11. Mandate expires | Rule `valid_until` passes → agent rules rejected |
| 12. Keep payments cheap | Keeper: `fosay-mandate-policy.extend_ttl`, `fosay-approvals.expire`, plus `ExtendFootprintTTL` on the account's own entries |
| 13. Activity feed / receipts | Indexer: SAC `transfer` events where `from` = wallet, attributed to an agent through the tx envelope's `AuthPayload.context_rule_ids`; `Mandate*` and `Intent*` events |

No function exists that isn't on this list.

---

## 7. Validation status

| Assumption | Status |
|---|---|
| We can sign for OZ v0.8.0 smart accounts (AuthPayload, preimage-v1) | ✅ on testnet |
| x402 facilitator accepts a smart-account payer | ✅ when no policy emits events (S1) |
| Approval bound to `execute(id)` works with an OZ account | ✅ (S2b, fee 12,284) |
| Smart-account payment cost in steady state | ✅ about the same as a G-account (11,332 vs 10,887) |
| `fosay-mandate-policy` steady-state cost < 50,000 stroops | ⏳ Phase 8 measurement (acceptance criterion) |
| Hosted Built-on-Stellar facilitator + real testnet USDC | ⏳ Phase 8 (host not yet allowed in the environment) |
| Owner signing with WebAuthn passkeys end to end | ⏳ `fosay-app` (verifier contract already canonical) |
| Upstream x402 issue (validate only asset events) | Deferred by decision, 2026-11-09 |

---

## 8. Verified facts (2026-11-09)

- `stellar-accounts` 0.8.0 → `soroban-sdk ^28.0.0`; `soroban-sdk` latest stable 29.0.1; Stellar CLI latest 30.0.1. Testnet runs protocol 30.
- OZ limits: 15 signers/rule, 5 policies/rule, rule name ≤ 20 bytes. One rule id per auth context.
- The canonical account exposes `execute(target, target_fn, target_args)`, `upgrade(new_wasm_hash, operator)`, `batch_add_signer`; no multi-rule batch.
- SAC `transfer(from: Address, to: MuxedAddress, amount: i128)` in soroban-sdk 28; `MuxedAddress::address()` exists.
- x402 facilitator (`@x402/stellar` 2.30.0): default fee ceiling 50,000 stroops; rejects any contract event other than the asset `transfer`; rejects auth sub-invocations; payment auth expiry ≤ ~12 ledgers (+2).
- Testnet: WebAuthn verifier `CC7EKIHQP3TN4CARQDND6CEOY2UXLWWC2X5GHTD5NLAT7BG5GPZIOM3F`, Ed25519 verifier `CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4`.
- Mainnet: WebAuthn verifier `CB7HENHJ7NF34I5FFXQK7D5I3WWQRGB5O5XO77D3NXMT7LM7LOKRQ5YR`, Ed25519 verifier `CBOOZV2BK5OETGL4Q4KGEBESPRLJFN7DOFWDT7OZGLD7EQEZUVOWUEMC`.
- Test fixtures in `tests/fixtures/` were fetched from testnet; their SHA-256 values match the smart-account-kit deployment manifest: account `c514d35e…`, Ed25519 verifier `60e8798d…`, WebAuthn verifier `e63a030d…`.
- smart-account-kit 0.10.0 and its bindings have not had an independent audit (their own READMEs).

Re-verify all contract IDs before mainnet.
