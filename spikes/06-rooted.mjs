// Spike S2b: owner auth rooted at the queue call (from.require_auth() then transfer).
// Expect: one auth entry rooted at pay_rooted with transfer as sub-invocation,
// two auth contexts -> context_rule_ids must have 2 entries; no non-root mode needed.
import fs from 'node:fs';
import { S, state, save, key, invoke, addr, i128, signSmartAccountEntry, simulateOnly } from './common.mjs';
const { Operation, Address, hash } = S;
const funder = key('funder'), owner = key('ownerKey'), agent = key('agentKey'), payee = key('payee');
const wasm = fs.readFileSync(new URL('./s2-contract/target/wasm32v1-none/release/spike_pay.wasm', import.meta.url));

if (!state.payContract2) {
  await invoke(funder, null, null, null, { op: Operation.uploadContractWasm({ wasm }) }).catch((e) => { if (!String(e).includes('Existing')) throw e; });
  const r = await invoke(funder, null, null, null, { op: Operation.createCustomContract({ address: new Address(funder.publicKey()), wasmHash: hash(wasm), salt: hash(Buffer.from('spike-pay-v2')) }) });
  state.payContract2 = r.result; save();
}
console.log('rooted spike contract', state.payContract2);

const call = (amount, ruleIds, signer, label) =>
  invoke(funder, state.payContract2, 'pay_rooted', [addr(state.sac), addr(state.account), addr(payee.publicKey()), i128(amount)], {
    authSigner: (e, exp) => {
      const r = e.rootInvocation(), f = r.function().contractFn();
      const subs = r.subInvocations().map((s) => s.function().contractFn().functionName().toString());
      console.log(`  [${label}] root: ${f.functionName().toString()} subs: [${subs}]`);
      signSmartAccountEntry(e, { ruleIds, signers: [signer], expLedger: exp });
    },
  }).then((r) => `OK fee ${r.fee} tx ${r.txHash}`, (e) => `REJECTED: ${String(e).split('\n')[0].slice(0, 140)}`);

console.log('1) owner approves 7 FUSD, rules [0,0], default record mode ->', await call(7_0000000n, [0, 0], owner, 'owner'));
console.log('2) owner with only one rule id [0] (length mismatch) ->', await call(7_0000000n, [0], owner, 'owner-1id'));
console.log('balance', String(await simulateOnly(funder, state.sac, 'balance', [addr(state.account)])));
