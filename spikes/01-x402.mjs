// Spike S1 + S4: x402 "exact" payment where the payer is an OZ smart account (C-address),
// authorized by the AGENT key under the agent rule, verified and settled by the
// official @x402/stellar facilitator code (run locally with our own facilitator key).
import { S, server, NET, state, key, addr, i128, signSmartAccountEntry, simulateOnly, diag, j } from './common.mjs';
import { ExactStellarScheme } from '@x402/stellar/exact/facilitator';
import { createEd25519Signer } from '@x402/stellar';
const { TransactionBuilder, Operation, rpc, Account } = S;

const agent = key('agentKey'), owner = key('ownerKey'), payee = key('payee'), facilitatorKp = key('facilitator'), funder = key('funder');
const { account, sac, agentRuleId } = state;
const NULL_ACCOUNT = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';

// Client side: what a FOSAY x402 signer must do (the stock client can't).
async function buildPayment(amount, { signer = agent, ruleId = agentRuleId } = {}) {
  const op = Operation.invokeContractFunction({ contract: sac, function: 'transfer', args: [addr(account), addr(payee.publicKey()), i128(amount)] });
  let tx = new TransactionBuilder(new Account(NULL_ACCOUNT, '0'), { fee: '100', networkPassphrase: NET }).addOperation(op).setTimeout(60).build();
  let sim = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) throw new Error('record sim failed: ' + diag(sim));
  const auth = sim.result.auth;
  const latest = (await server.getLatestLedger()).sequence;
  for (const e of auth) signSmartAccountEntry(e, { ruleIds: [ruleId], signers: [signer], expLedger: latest + 10 });
  const func = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
  tx = new TransactionBuilder(new Account(NULL_ACCOUNT, '0'), { fee: '100', networkPassphrase: NET })
    .addOperation(Operation.invokeHostFunction({ func, auth })).setTimeout(60).build();
  sim = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) return { error: diag(sim) };
  return { xdr: rpc.assembleTransaction(tx, sim).build().toXDR(), minResourceFee: Number(sim.minResourceFee), authEntries: auth.length };
}

const reqs = (amount) => ({ scheme: 'exact', network: 'stellar:testnet', asset: sac, payTo: payee.publicKey(), amount: String(amount), maxTimeoutSeconds: 60, extra: { areFeesSponsored: true } });
const facilitator = new ExactStellarScheme([createEd25519Signer(facilitatorKp.secret(), 'stellar:testnet')]);
const bal = async (a) => String(await simulateOnly(funder, sac, 'balance', [addr(a)]));

console.log('== S1: agent pays 1 FUSD via x402 from the smart account');
console.log('  before: account', await bal(account), 'payee', await bal(payee.publicKey()));
const p = await buildPayment(1_0000000n);
if (p.error) { console.log('  CLIENT FAILED:', p.error); process.exit(1); }
console.log(`  client payload built: ${p.authEntries} auth entry, minResourceFee ${p.minResourceFee} stroops`);
const payload = { x402Version: 2, accepted: reqs(1_0000000n), payload: { transaction: p.xdr } };
const v = await facilitator.verify(payload, reqs(1_0000000n));
console.log('  facilitator.verify (default 50,000 stroop ceiling):', j(v));
let settleFacilitator = facilitator;
if (!v.isValid && v.invalidReason === 'invalid_exact_stellar_payload_fee_exceeds_maximum') {
  console.log('  -> fee ceiling hit; retrying with 500,000 ceiling to test the rest of the flow');
  settleFacilitator = new ExactStellarScheme([createEd25519Signer(facilitatorKp.secret(), 'stellar:testnet')], { maxTransactionFeeStroops: 500000 });
  console.log('  verify (raised ceiling):', j(await settleFacilitator.verify(payload, reqs(1_0000000n))));
}
const s = await settleFacilitator.settle(payload, reqs(1_0000000n));
console.log('  facilitator.settle:', j(s));
console.log('  after:  account', await bal(account), 'payee', await bal(payee.publicKey()));
if (s.success) {
  const t = await server.getTransaction(s.transaction);
  console.log('  S4 fee actually charged to facilitator (stroops):', t.resultXdr?.feeCharged().toString());
}

console.log('\n== Negative: agent tries 6 FUSD (limit 5 per window)');
const over = await buildPayment(6_0000000n);
console.log('  ', over.error ? 'REJECTED at simulation: ' + over.error.split('\n')[0] : 'UNEXPECTEDLY ALLOWED');

console.log('\n== Negative: owner key presented under the agent rule');
const wrong = await buildPayment(1_0000000n, { signer: owner });
console.log('  ', wrong.error ? 'REJECTED at simulation: ' + wrong.error.split('\n')[0] : 'UNEXPECTEDLY ALLOWED');
