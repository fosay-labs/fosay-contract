// Isolate the x402 rejection: same payment, but under an agent rule WITHOUT policies.
import { S, server, NET, state, save, key, invoke, addr, i128, someU32, vec, ctxCallContract, externalSigner,
  ED25519_VERIFIER, signSmartAccountEntry, simulateOnly, diag, j } from './common.mjs';
import { ExactStellarScheme } from '@x402/stellar/exact/facilitator';
import { createEd25519Signer } from '@x402/stellar';
const { TransactionBuilder, Operation, rpc, Account, xdr } = S;

const agent = key('agentKey'), owner = key('ownerKey'), payee = key('payee'), facilitatorKp = key('facilitator'), funder = key('funder');
const { account, sac } = state;
const NULL_ACCOUNT = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';

if (state.rawRuleId == null) {
  const latest = (await server.getLatestLedger()).sequence;
  const r = await invoke(funder, account, 'add_context_rule',
    [ctxCallContract(sac), S.nativeToScVal('agt1:raw', { type: 'string' }), someU32(latest + 100000), vec([externalSigner(ED25519_VERIFIER, agent)]), xdr.ScVal.scvMap([])],
    { authSigner: (e, exp) => signSmartAccountEntry(e, { ruleIds: [0], signers: [owner], expLedger: exp }) });
  state.rawRuleId = r.result.id; save();
}
console.log('raw agent rule id', state.rawRuleId);

async function build(amount, ruleId) {
  const op = Operation.invokeContractFunction({ contract: sac, function: 'transfer', args: [addr(account), addr(payee.publicKey()), i128(amount)] });
  let tx = new TransactionBuilder(new Account(NULL_ACCOUNT, '0'), { fee: '100', networkPassphrase: NET }).addOperation(op).setTimeout(60).build();
  let sim = await server.simulateTransaction(tx);
  const auth = sim.result.auth;
  const latest = (await server.getLatestLedger()).sequence;
  for (const e of auth) signSmartAccountEntry(e, { ruleIds: [ruleId], signers: [agent], expLedger: latest + 10 });
  const func = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
  tx = new TransactionBuilder(new Account(NULL_ACCOUNT, '0'), { fee: '100', networkPassphrase: NET }).addOperation(Operation.invokeHostFunction({ func, auth })).setTimeout(60).build();
  sim = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) throw new Error(diag(sim));
  const events = (sim.events || []).map((d) => d.event()).filter((e) => e.type().name === 'contract')
    .map((e) => `${S.StrKey.encodeContract(e.contractId())} ${e.body().v0().topics().map((t) => { try { return String(S.scValToNative(t)); } catch { return '?'; } }).join(',')}`);
  return { xdr: rpc.assembleTransaction(tx, sim).build().toXDR(), fee: Number(sim.minResourceFee), events };
}

for (const [label, ruleId] of [['WITH spending_limit (rule ' + state.agentRuleId + ')', state.agentRuleId], ['NO policies (rule ' + state.rawRuleId + ')', state.rawRuleId]]) {
  const b = await build(1_0000000n, ruleId);
  console.log(`\n== ${label}: minResourceFee ${b.fee}\n   contract events in simulation:\n   - ${b.events.join('\n   - ')}`);
}

const r = { scheme: 'exact', network: 'stellar:testnet', asset: sac, payTo: payee.publicKey(), amount: '10000000', maxTimeoutSeconds: 60, extra: { areFeesSponsored: true } };
const b = await build(1_0000000n, state.rawRuleId);
const payload = { x402Version: 2, accepted: r, payload: { transaction: b.xdr } };
for (const ceiling of [50000, 500000]) {
  const f = new ExactStellarScheme([createEd25519Signer(facilitatorKp.secret(), 'stellar:testnet')], { maxTransactionFeeStroops: ceiling });
  const v = await f.verify(payload, r);
  console.log(`\nverify, no-policy rule, ceiling ${ceiling}:`, j(v));
  if (v.isValid) {
    const s = await f.settle(payload, r);
    console.log('settle:', j(s));
    if (s.success) {
      const t = await server.getTransaction(s.transaction);
      console.log('fee charged (stroops):', t.resultXdr?.feeCharged().toString());
      console.log('payee balance now:', String(await simulateOnly(funder, sac, 'balance', [addr(payee.publicKey())])));
    }
    break;
  }
}
