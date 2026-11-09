# Test fixtures: canonical OpenZeppelin smart-account contracts

Fetched from Stellar testnet on 2026-11-09. They are byte-identical to the
contracts every smart-account-kit wallet uses, so FOSAY's integration tests
run against the real account code, not a mock.

| File | SHA-256 (= network WASM hash) | Source |
|---|---|---|
| `oz_smart_account_v0_8_0.wasm` | `c514d35ea67eaea9bfc1fb3327854c0c06111bc1ddebd4fe217c504c1004b9e1` | OZ `stellar-accounts` v0.8.0 `multisig-account-example` |
| `oz_ed25519_verifier.wasm` | `60e8798db610bdaf3370d39ebda56ee1dc2c15ce1c3a9e28b528bfa24a06b477` | testnet `CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4` |
| `oz_webauthn_verifier.wasm` | `e63a030d0f1a1481e36059a4837c433083b33e704c1f9625b7314795b6d72b76` | testnet `CC7EKIHQP3TN4CARQDND6CEOY2UXLWWC2X5GHTD5NLAT7BG5GPZIOM3F` |

The hashes match `stellar/smart-account-kit` `docs/deployments-*.md`. To check them:

```bash
sha256sum tests/fixtures/*.wasm
```

Re-fetch with `stellar contract fetch --wasm-hash <hash> --network testnet -o <file>`
or `stellar contract fetch --id <contract-id> --network testnet -o <file>`.
