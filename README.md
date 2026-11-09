# fosay-contract
Soroban contracts for FOSAY: mandate policy and approval queue for AI-agent smart wallets on Stellar.

> **F**unds **O**nly **S**pent **A**fter **Y**es. Your AI agent gets its own key on your
> smart wallet with a budget you set. Small payments go through on their own; bigger
> ones wait for you to approve them with your passkey.

## Contracts

| Crate | What it does |
|---|---|
| [`fosay-mandate-policy`](contracts/mandate-policy) | OpenZeppelin smart-account policy on the agent's `:pay` rule: per-payment cap, constant-size rolling budget, recipient allowlist, pause, mandate hash. Emits no events while enforcing, so payments stay x402-compatible. |
| [`fosay-approvals`](contracts/approvals) | Queue of over-limit payment requests. The agent proposes; the owner approves with `execute(id)`, authorized by the wallet before the token moves. |

The wallet itself is the canonical OpenZeppelin `stellar-accounts` v0.8.0 smart account
(deployed per user by smart-account-kit). FOSAY doesn't ship its own account contract.

Design: [`docs/SPEC.md`](docs/SPEC.md) · Testnet evidence: [`docs/SPIKES.md`](docs/SPIKES.md)

## Requirements

- Rust stable with the `wasm32v1-none` target (pinned in `rust-toolchain.toml`)
- Stellar CLI ≥ 25.2.0 for wasm builds: `cargo install --locked stellar-cli@30.0.1`

soroban-sdk 28 must be built to wasm through `stellar contract build`; a plain
`cargo build --target wasm32v1-none` is rejected. Native `cargo test` works directly.

## Build

```bash
stellar contract build
# target/wasm32v1-none/release/fosay_mandate_policy.wasm
# target/wasm32v1-none/release/fosay_approvals.wasm
```

## Test

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Integration tests deploy the **canonical** OpenZeppelin smart account and Ed25519
verifier from [`tests/fixtures/`](tests/fixtures) (byte-identical to the deployed
contracts) and authorize every call with real Ed25519 signatures. They cover the full
flow: the agent pays, hits its budget, asks; the owner approves, pauses, and revokes.

## Layout

```
contracts/mandate-policy/   fosay-mandate-policy (policy + tests)
contracts/approvals/        fosay-approvals (queue + tests, incl. end-to-end)
tests/fixtures/             canonical OZ smart account and verifier wasm
docs/                       SPEC, SPIKES, DEPLOYMENT, coding-agent system prompt
scripts/                    deploy + testnet smoke test (Node)
deployments/                deployed contract IDs per network
spikes/                     throwaway testnet scripts behind SPIKES.md
```

## Status

Not audited. Deployed to **testnet**; IDs and measured costs are in [`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md).

## License

Apache-2.0
