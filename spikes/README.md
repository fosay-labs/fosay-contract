# FOSAY spikes (testnet only)

Throwaway scripts that tested the design assumptions. Results: [`../docs/SPIKES.md`](../docs/SPIKES.md).

```bash
npm install
cd s2-contract && stellar contract build && cd ..
# Node's fetch needs NODE_USE_ENV_PROXY=1 only behind an HTTPS proxy
node 00-setup.mjs        # fund keys, test token, deploy OZ smart account, add agent rule
node 01-x402.mjs         # S1/S4: x402 payment from the smart account (agent key)
node 02-x402-nopolicy.mjs# S1: isolate the policy-event rejection; settle without policies
node 03-baseline.mjs     # S4: G-account baseline with the stock x402 client
node 04-direct-limit.mjs # S4: steady-state cost with OZ spending_limit
node 05-nonroot.mjs      # S2a: non-root auth through a queue-like contract
node 06-rooted.mjs       # S2b: rooted auth (the design FOSAY adopts)
```

`state.json` holds generated **testnet** secret keys and is git-ignored. Never reuse them.
