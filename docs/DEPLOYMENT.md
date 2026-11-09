# FOSAY — Deployment

## Current testnet deployment (2026-11-09)

| Contract | ID | WASM hash | Source |
|---|---|---|---|
| `fosay-mandate-policy` | `CABMDEJEXOIBR4F6TDIYQ7CA5GFENXAYDRA4WH7V4QR7PAMTFIEQWWQ5` | `46336ed2e9d1f05b82b7331309fd5c6123727f70d20120a14e5898a1ba39cd70` | `d0bb63f` |
| `fosay-approvals` | `CAZDF7XQSKLZK5JG4FPCG6ZSGK3C4ZGNBKHU3VYDPQSZWLX2HPRXJCIU` | `867499c4eece48777833fc28d58e63c5f904dbc7c1d73c1a043c72cc24f44fe2` | `fb82561` (unchanged since) |

`fosay-approvals` was deployed with `max_pending_per_account = 20`, `max_ttl_ledgers = 120,960` (~7 days).
Machine-readable record: [`deployments/testnet.json`](../deployments/testnet.json).
Smoke-test record: [`deployments/testnet-smoke.json`](../deployments/testnet-smoke.json).

Canonical contracts FOSAY relies on (not deployed by FOSAY):

| | Testnet |
|---|---|
| OZ smart account WASM (v0.8.0) | `c514d35ea67eaea9bfc1fb3327854c0c06111bc1ddebd4fe217c504c1004b9e1` |
| WebAuthn verifier | `CC7EKIHQP3TN4CARQDND6CEOY2UXLWWC2X5GHTD5NLAT7BG5GPZIOM3F` |
| Ed25519 verifier | `CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4` |

An earlier mandate-policy build (`CCQ4VOFX…MEBP`, WASM `fdee27e5…`) was deployed first. It
is superseded; don't use it (see "What Phase 8 changed" below).

## Deploy

```bash
stellar contract build
cd scripts && npm install
node deploy.mjs
```

- `STELLAR_NETWORK` defaults to `testnet`. Only testnet is configured.
- `STELLAR_DEPLOYER_SECRET`: optional on testnet. Without it, a key is created in
  `scripts/.secrets/deployer` (git-ignored, mode 0600) and funded by Friendbot.
- It's idempotent: the salt is `sha256("fosay:<crate>:<wasm-hash>")`, so the same build
  maps to the same contract ID and re-running skips anything already uploaded or deployed.
  A new build gets a new ID.
- It sanity-checks the live contracts and writes `deployments/<network>.json`, then prints
  an `.env` block for `fosay-app`.
- Behind an HTTPS proxy, Node's `fetch` needs `NODE_USE_ENV_PROXY=1` (Node ≥ 22.21) and,
  if the proxy re-signs TLS, `NODE_EXTRA_CA_CERTS=<bundle>`.

Equivalent Stellar CLI steps, if you prefer the CLI:

```bash
stellar contract upload --wasm target/wasm32v1-none/release/fosay_mandate_policy.wasm --source deployer --network testnet
stellar contract deploy --wasm-hash <hash> --source deployer --network testnet
stellar contract upload --wasm target/wasm32v1-none/release/fosay_approvals.wasm --source deployer --network testnet
stellar contract deploy --wasm-hash <hash> --source deployer --network testnet -- \
  --max_pending_per_account 20 --max_ttl_ledgers 120960
```

## Smoke test

```bash
cd scripts && node smoke-testnet.mjs
```

It creates a test token and a FOSAY wallet (canonical OZ account; an Ed25519 owner key
stands in for the passkey), connects an agent, runs the keeper step, pays twice via x402
through the official `@x402/stellar` facilitator **at its default settings**, checks the
refusals, then does the propose → owner-approve flow. Agent auth entries are signed with
smart-account-kit's `signAuthEntryWithSigners` (the path `fosay-app` uses). State lives
in `scripts/.secrets/` (git-ignored).

### Measured on testnet (fresh wallet, 2026-11-09)

| Step | Result | Resource fee / charged (stroops) |
|---|---|---|
| Connect agent: `:pay` rule (installs mandate) + `:ask` rule | ✅ | 771,801 + 92,505 charged (one-time) |
| Agent payment **before** keeper (simulated only) | would be rejected | 311,366 |
| Keeper: extend 12 footprint entries to 31 days | ✅ | 412,174 charged |
| x402 payment 1 (first ever), facilitator default 50k ceiling | ✅ verified + settled | 18,959 / 13,971 |
| x402 payment 2 | ✅ verified + settled | 18,959 / 13,971 |
| 3 FUSD (over 2 FUSD per-tx cap) | ✅ refused, decoded `4107 PerTxLimitExceeded` | — |
| 1.5 FUSD (over remaining budget) | ✅ refused, decoded `4109 BudgetExceeded` | — |
| Agent `propose` (smart-account-kit signature, `:ask` rule) | ✅ | 409,852 charged |
| Owner `execute(id)` with rule ids `[0, 0]` | ✅ Executed, merchant paid | 20,697 / 15,624 |

**The acceptance criterion is met:** a steady-state agent payment under the mandate
policy costs ~19k stroops (limit 50k), and the official facilitator verifies and
settles it, including the very first payment from a fresh wallet once the keeper has run.

## What Phase 8 changed

1. **The budget ring is now fixed-size** (`453ef0b`). The first deployed build stored
   the budget as a list that grew and shrank. The first payment into a fresh mandate cost
   54,322 vs 18,683 in steady state, because growing a persistent entry pays rent on the
   new bytes. That breaks the x402 ceiling and repeats whenever buckets are pruned and
   re-added. The ring is created full at install and written in place, so the entry size
   never changes. Redeployed as `CABMDEJE…WWQ5`.
2. **The keeper isn't optional.** New persistent entries on testnet live about 6 days.
   The OZ account extends any entry it reads with < 29 days left to 30 days, and the
   payment that triggers this pays the rent: ~250–345k stroops, rejected by x402. The
   keeper must keep every entry a payment touches above 29 days (the smoke test uses 31):
   the wallet instance, the agent's rule/signer/policy data, the mandate entry, the
   token balance, and the contract code entries.
3. **Shared code TTL:** on 2026-11-09 the canonical OZ account WASM (`c514d35e…`) had
   ~0 days of TTL left on testnet. If it's archived, every v0.8.0 wallet fails until
   someone restores it. The first keeper run extended it (and the verifier/policy code);
   that one-off cost ~37 XLM of testnet lumens because a 48 KB code entry is expensive to
   extend. The `fosay-app` keeper must watch these shared code entries too.
4. **x402 payment window:** sign payments with expiry `latest + ceil(maxTimeoutSeconds/5)`
   and use a generous `maxTimeoutSeconds` (120). A 10-ledger window expired between
   build and settlement in testing and the facilitator's transaction failed on-chain.

## Known follow-ups (not blocking)

- `propose` costs ~410k stroops because `Intent` and `PendingCount` entries are extended to
  30 days. Intents live at most `max_ttl_ledgers` (7 days) as `Pending`, so extending to
  that horizon plus a margin would cut the cost; history belongs in the indexer.
- Hosted Built-on-Stellar facilitator + real testnet USDC: not yet tested (host not
  allowed in this environment; USDC comes from a browser faucet). The smoke test runs the
  same facilitator code locally with a test asset.
- Owner signing with a real WebAuthn passkey happens in `fosay-app` (the verifier is
  canonical; the smoke test uses an Ed25519 owner key).
