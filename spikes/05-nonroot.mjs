// Spike S2: a contract that calls token.transfer(from=smart account) WITHOUT itself
// requiring auth (the fosay-approvals::execute pattern). The owner authorizes only
// the inner transfer (non-root auth entry). Over-limit amounts must need the owner.
import fs from 'node:fs';
import { S, server, state, save, key, invoke, addr, i128, signSmartAccountEntry, simulateOnly } from './common.mjs';
const { Operation, Address, hash } = S;

const funder = key('funder'), owner = key('ownerKey'), agent = key('agentKey'), payee = key('payee');
const wasm = fs.readFileSync(new URL('./s2-contract/target/wasm32v1-none/release/spike_pay.wasm', import.meta.url));

if (!state.payContract) {
  await invoke(funder, null, null, null, { op: Operation.uploadContractWasm({ wasm }) }).catch((e) => { if (!String(e).includes('Existing')) throw e; });
  const r = await invoke(funder, null, null, null, { op: Operation.createCustomContract({ address: new Address(funder.publicKey()), wasmHash: hash(wasm), salt: hash(Buffer.from('spike-pay-v1')) }) });
  state.payContract = r.result; save();
}
console.log('spike pay contract', state.payContract);
const bal = async () => String(await simulateOnly(funder, state.sac, 'balance', [addr(state.account)]));

const call = (amount, ruleId, signer, label) =>
  invoke(funder, state.payContract, 'pay_from', [addr(state.sac), addr(state.account), addr(payee.publicKey()), i128(amount)], {
    recordMode: 'record_allow_nonroot',
    authSigner: (e, exp) => {
      const inv = e.rootInvocation().function().contractFn();
      console.log(`  [${label}] auth entry root invocation: ${S.StrKey.encodeContract(inv.contractAddress().contractId())}.${inv.functionName().toString()} (sub-invocations: ${e.rootInvocation().subInvocations().length})`);
      signSmartAccountEntry(e, { ruleIds: [ruleId], signers: [signer], expLedger: exp });
    },
  }).then((r) => `OK fee ${r.fee} tx ${r.txHash}`, (e) => `REJECTED: ${String(e).split('\n')[0].slice(0, 160)}`);

console.log('balance before', await bal());
console.log('1) owner approves 7 FUSD (over agent limit) under owner rule 0 ->', await call(7_0000000n, 0, owner, 'owner'));
console.log('2) agent tries 7 FUSD under its own rule ->', await call(7_0000000n, state.agentRuleId, agent, 'agent'));
console.log('3) agent pays 0.5 FUSD under its own rule via the contract ->', await call(5000000n, state.agentRuleId, agent, 'agent-small'));
console.log('balance after', await bal());
