// Steady-state cost of an agent payment under the spending_limit rule (direct, not x402).
import { state, key, invoke, addr, i128, signSmartAccountEntry, simulateOnly } from './common.mjs';
const agent = key('agentKey'), funder = key('funder'), payee = key('payee');
for (let i = 1; i <= 3; i++) {
  const r = await invoke(funder, state.sac, 'transfer', [addr(state.account), addr(payee.publicKey()), i128(5000000n)],
    { authSigner: (e, exp) => signSmartAccountEntry(e, { ruleIds: [state.agentRuleId], signers: [agent], expLedger: exp }) });
  console.log(`payment ${i}: 0.5 FUSD, minResourceFee ${r.minResourceFee}, fee charged ${r.fee}`);
}
console.log('account balance', String(await simulateOnly(funder, state.sac, 'balance', [addr(state.account)])));
