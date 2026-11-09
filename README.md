<p align="center"><img src=".github/assets/banner.svg" alt="FOSAY: Funds Only Spent After Yes" width="100%"></p>

# fosay-contract

[![CI](https://github.com/fosay-labs/fosay-contract/actions/workflows/ci.yml/badge.svg)](https://github.com/fosay-labs/fosay-contract/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Network: Stellar testnet](https://img.shields.io/badge/network-Stellar%20testnet-7d00ff)](https://stellar.expert/explorer/testnet/contract/CABMDEJEXOIBR4F6TDIYQ7CA5GFENXAYDRA4WH7V4QR7PAMTFIEQWWQ5)
[![soroban-sdk 28](https://img.shields.io/badge/soroban--sdk-28.0.0-black)](https://crates.io/crates/soroban-sdk)
[![Good first issues](https://img.shields.io/github/issues/fosay-labs/fosay-contract/good%20first%20issue?label=good%20first%20issues&color=7057ff)](https://github.com/fosay-labs/fosay-contract/labels/good%20first%20issue)

Soroban contracts that let an AI agent spend from your Stellar smart wallet **within a
budget you set**. Anything bigger waits for you to approve it with your passkey.

> **Status: testnet, not audited.** 48 tests pass, and the contracts are deployed to testnet and
> exercised end to end by [`fosay-app`](https://github.com/fosay-labs/fosay-app), including
> x402 payments settled by the official facilitator. Don't use them with real funds.

**Try it (testnet):** [fosay-labs.github.io/fosay-app](https://fosay-labs.github.io/fosay-app/) ·
**App, SDK and MCP server:** [`fosay-labs/fosay-app`](https://github.com/fosay-labs/fosay-app) ·
**Docs:** [fosay-labs.github.io/fosay-app/docs](https://fosay-labs.github.io/fosay-app/docs/) ·
**Spec:** [`docs/SPEC.md`](docs/SPEC.md) · **Deployment:** [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md)

## How it works

The wallet is the canonical OpenZeppelin `stellar-accounts` v0.8.0 smart account. FOSAY doesn't
ship its own account contract. It adds two contracts and three context rules:

| Rule | Scope | Signer | Policy | What it allows |
|---|---|---|---|---|
| `0` owner | `Default` | owner's passkey | none | everything, including approving requests |
| `agt:pay` | `CallContract(USDC)` | agent's Ed25519 key | **`fosay-mandate-policy`** | `transfer` within the mandate |
| `agt:ask` | `CallContract(fosay-approvals)` | agent's Ed25519 key | none | `propose` / `cancel` its own requests |

```
agent pays 1 USDC ─▶ account.__check_auth ─▶ rule agt:pay ─▶ mandate-policy.enforce
                                                               ├─ paused?           → Paused
                                                               ├─ signer set ok? (I3)
                                                               ├─ transfer on USDC?  → NotAllowed
                                                               ├─ ≤ max per tx?      → PerTxLimitExceeded
                                                               ├─ recipient allowed? → RecipientNotAllowed
                                                               └─ window + amount ≤ budget? → BudgetExceeded
agent wants 30 USDC ─▶ approvals.propose ─▶ owner's phone ─▶ passkey signs execute(id) ─▶ transfer
```

| Contract | Testnet ID | Entry points |
|---|---|---|
| [`fosay-mandate-policy`](contracts/mandate-policy) | [`CABMDEJE…WWQ5`](https://stellar.expert/explorer/testnet/contract/CABMDEJEXOIBR4F6TDIYQ7CA5GFENXAYDRA4WH7V4QR7PAMTFIEQWWQ5) | `install`, `enforce`, `uninstall` (called by the account), `set_paused`, `update_mandate`, `extend_ttl`, `get_mandate`, `remaining_budget` |
| [`fosay-approvals`](contracts/approvals) | [`CAZDF7XQ…XJCIU`](https://stellar.expert/explorer/testnet/contract/CAZDF7XQSKLZK5JG4FPCG6ZSGK3C4ZGNBKHU3VYDPQSZWLX2HPRXJCIU) | `__constructor` (fixed config, set once), `propose`, `execute`, `cancel`, `expire`, `get_intent`, `pending_count` |

Rules the code doesn't bend (full list: SPEC §2, mapped to tests in [`SECURITY.md`](SECURITY.md#threat-model)):

- The agent's key can't change rules, unpause itself or edit its mandate (**I1**).
- No FOSAY contract moves funds on its own authority (**I2**), and neither has an admin or upgrade path (**I4**).
- An owner approval is bound to `execute(id)`. It can't be replayed as a bare transfer (**I6**).
- `enforce` emits no events, so agent payments pass x402 facilitator checks (**I7**).
- The rolling budget uses a fixed 9-slot ring, so storage and fees don't grow with use.

## Quick start

Requirements: Rust stable with `wasm32v1-none` (pinned in `rust-toolchain.toml`) and the
Stellar CLI.

```bash
cargo install --locked stellar-cli@30.0.1 --no-default-features
```

```bash
cargo test --workspace
```

```bash
stellar contract build
```

soroban-sdk 28 must be built to wasm through `stellar contract build`. Native `cargo test` works
without it. The integration tests deploy the canonical OZ account and verifier from
[`tests/fixtures/`](tests/fixtures) (byte-identical to the deployed ones; CI checks the hashes)
and sign every call with real Ed25519 keys.

Deploy your own copy: [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md) (`scripts/deploy.mjs` is idempotent).

## Repository layout

```
contracts/mandate-policy/   policy contract + tests (budget property test, invariants I1/I3/I7)
contracts/approvals/        request queue + tests (incl. end-to-end flow)
tests/fixtures/             canonical OZ smart account and verifier wasm
scripts/                    deploy + testnet smoke test (Node)
deployments/                contract IDs per network
docs/                       SPEC, SPIKES (testnet evidence), DEPLOYMENT, coding-agent prompt
```

## Roadmap

Planned work is tracked as [issues](https://github.com/fosay-labs/fosay-contract/issues). Issues
labelled [`wave`](https://github.com/fosay-labs/fosay-contract/labels/wave) are scoped for
Stellar Wave contributors, with `tier/trivial`, `tier/medium` or `tier/high` complexity.
Mainnet comes after an external audit.

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) first: the security invariants are binding, and
authorization tests use real signatures. Claim an issue by commenting on it.
Report vulnerabilities privately; see [`SECURITY.md`](SECURITY.md).

## Maintainers

| Maintainer | Role | GitHub |
|---|---|---|
| ciscokwiz | Lead maintainer (contracts, app) | [@ciscokwiz](https://github.com/ciscokwiz) |

Questions and ideas: [GitHub Discussions](https://github.com/fosay-labs/fosay-contract/discussions).

## Contributors

[![Contributors](https://contrib.rocks/image?repo=fosay-labs/fosay-contract)](https://github.com/fosay-labs/fosay-contract/graphs/contributors)

## License

[Apache-2.0](LICENSE)
