# Changelog

All notable changes to this project. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [SemVer](https://semver.org/). Contracts are immutable (SPEC I4), so a new
contract version is a new deployment with new IDs.

## [Unreleased]

## [0.1.0-testnet] - 2026-11-09

First testnet release.

### Added
- `fosay-mandate-policy`: OpenZeppelin smart-account policy for an agent's `:pay` rule.
  Per-payment cap, rolling budget in a fixed 9-slot ring, recipient allowlist, pause, mandate
  hash. `enforce` emits no events (x402-compatible).
- `fosay-approvals`: over-limit request queue (`propose`, `execute`, `cancel`, `expire`, views)
  with owner approvals bound to `execute(id)`.
- 48 tests, including end-to-end flows on the canonical OZ account with real Ed25519 signatures.
- Idempotent deploy script and testnet smoke test.

### Deployed (testnet)
- `fosay-mandate-policy` `CABMDEJEXOIBR4F6TDIYQ7CA5GFENXAYDRA4WH7V4QR7PAMTFIEQWWQ5`
- `fosay-approvals` `CAZDF7XQSKLZK5JG4FPCG6ZSGK3C4ZGNBKHU3VYDPQSZWLX2HPRXJCIU`

[Unreleased]: https://github.com/fosay-labs/fosay-contract/compare/v0.1.0-testnet...HEAD
[0.1.0-testnet]: https://github.com/fosay-labs/fosay-contract/releases/tag/v0.1.0-testnet
