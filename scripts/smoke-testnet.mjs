// End-to-end smoke test against the deployed FOSAY contracts (deployments/<network>.json).
//
// Creates a FOSAY wallet (canonical OZ v0.8.0 account; an Ed25519 owner key stands in for
// the passkey), connects an agent (:pay rule with fosay-mandate-policy, :ask rule on
// fosay-approvals), then:
//   0. keeper extends the TTL of everything a payment touches (wallet entries, mandate,
//      wallet + verifier + policy code) so payments run in the cheap steady state
//   A. agent pays 2× via x402, verified + settled by the official @x402/stellar facilitator
//      with its DEFAULT settings (50,000-stroop fee ceiling); agent entries are signed with
//      smart-account-kit's signAuthEntryWithSigners
//   B. over-cap and over-budget payments are refused, with the policy error decoded
//   C. agent proposes the over-budget payment; owner approves execute(id) with rule ids [0, 0]
// Results go to deployments/<network>-smoke.json.
import fs from 'node:fs';
import path from 'node:path';
import { ExactStellarScheme } from '@x402/stellar/exact/facilitator';
import { createEd25519Signer } from '@x402/stellar';
import { Ed25519Signer, KNOWN_AUTH_DIGEST_SCHEMES, signAuthEntryWithSigners } from 'smart-account-kit';
import {
  S, network, loadKey, ensureFunded, invoke, view, signOwnerEntry, diag, json, extendFootprint,
  addr, i128, u32, u64, str, bytes, vec, smap, someU32, externalSigner, ctxCallContract, REPO, SECRETS_DIR,
} from './lib.mjs';

const { Operation, Asset, Address, TransactionBuilder, Account, rpc, hash, xdr } = S;
const UNIT = 10_000_000n;
const NULL_ACCOUNT = 'GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF';
// x402 payment window. The facilitator accepts auth expiry up to
// latest + ceil(maxTimeoutSeconds / ~5s) + 2. Leave headroom: build + verify + settle
// can take tens of seconds (a 10-ledger window expired mid-settle in testing).
const MAX_TIMEOUT_SECONDS = 120;
const PAYMENT_EXPIRY_LEDGERS = Math.ceil(MAX_TIMEOUT_SECONDS / 5);

const net = network();
const dep = JSON.parse(fs.readFileSync(path.join(REPO, `deployments/${net.name}.json`), 'utf8'));
const POLICY = dep.contracts.mandatePolicy.id;
const APPROVALS = dep.contracts.approvals.id;
const statePath = path.join(SECRETS_DIR, 'smoke-state.json');
const state = fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')) : {};
const save = () => fs.writeFileSync(statePath, JSON.stringify(state, null, 2), { mode: 0o600 });

const funder = loadKey('deployer', 'STELLAR_DEPLOYER_SECRET');
const issuer = loadKey('smoke-issuer');
const merchant = loadKey('smoke-merchant');
const facilitatorKp = loadKey('smoke-facilitator');
const owner = loadKey('smoke-owner');
const agent = loadKey('smoke-agent');
const report = { network: net.name, at: new Date().toISOString(), contracts: { POLICY, APPROVALS }, steps: [] };
const step = (name, data) => {
  report.steps.push({ name, ...data });
  console.log(`✓ ${name}`, json(data));
};

for (const k of [funder, issuer, merchant, facilitatorKp]) await ensureFunded(net, k);
const latest = async () => (await net.server.getLatestLedger()).sequence;

async function classic(src, ops) {
  let b = new TransactionBuilder(await net.server.getAccount(src.publicKey()), { fee: '1000', networkPassphrase: net.passphrase });
  for (const o of ops) b = b.addOperation(o);
  const tx = b.setTimeout(120).build();
  tx.sign(src);
  const r = await net.server.sendTransaction(tx);
  if (r.status === 'ERROR') throw new Error('classic tx failed');
  await net.server.pollTransaction(r.hash, { attempts: 40 });
}

// ---- setup: token, wallet, agent rules ----
const asset = new Asset('FUSD', issuer.publicKey());
const token = asset.contractId(net.passphrase);
if (!state.token) {
  await invoke(net, funder, { op: Operation.createStellarAssetContract({ asset }) }).catch((e) => {
    if (!/ExistingValue|already exists/i.test(String(e))) throw e;
  });
  await classic(merchant, [Operation.changeTrust({ asset })]);
  state.token = token;
  save();
}

const ownerSign = (ruleIds) => (entry, exp) =>
  signOwnerEntry(net, entry, { ruleIds, signers: [owner], expLedger: exp, verifier: net.ed25519Verifier });

if (!state.wallet) {
  const r = await invoke(net, funder, {
    op: Operation.createCustomContract({
      address: new Address(funder.publicKey()),
      wasmHash: Buffer.from(net.accountWasmHash, 'hex'),
      salt: hash(Buffer.from(`fosay-smoke:${owner.publicKey()}`)),
      constructorArgs: [vec([externalSigner(net.ed25519Verifier, owner)]), xdr.ScVal.scvMap([])],
    }),
  });
  state.wallet = r.result;
  save();
  await invoke(net, issuer, { contract: token, fn: 'mint', args: [addr(state.wallet), i128(1_000n * UNIT)] });
}
const wallet = state.wallet;

const mandateDoc = { v: 1, agentName: 'smoke-agent', purpose: 'testnet smoke test', budget: '50000000', maxPerTx: '20000000', periodLedgers: 17280 };
const mandateHash = hash(Buffer.from(JSON.stringify(mandateDoc)));
if (state.payRule == null) {
  const params = smap([
    ['budget', i128(5n * UNIT)],
    ['mandate_hash', bytes(mandateHash)],
    ['max_per_tx', i128(2n * UNIT)],
    ['period_ledgers', u32(17_280)],
    ['recipients', vec([])],
  ]);
  const policies = xdr.ScVal.scvMap([new xdr.ScMapEntry({ key: addr(POLICY), val: params })]);
  const validUntil = (await latest()) + 30 * 17_280;
  const pay = await invoke(net, funder, {
    contract: wallet, fn: 'add_context_rule',
    args: [ctxCallContract(token), str('agt1:pay'), someU32(validUntil), vec([externalSigner(net.ed25519Verifier, agent)]), policies],
    authSigner: ownerSign([0]),
  });
  state.payRule = pay.result.id;
  const ask = await invoke(net, funder, {
    contract: wallet, fn: 'add_context_rule',
    args: [ctxCallContract(APPROVALS), str('agt1:ask'), someU32(validUntil), vec([externalSigner(net.ed25519Verifier, agent)]), xdr.ScVal.scvMap([])],
    authSigner: ownerSign([0]),
  });
  state.askRule = ask.result.id;
  save();
  step('connect agent (2 owner signatures)', {
    wallet, payRule: state.payRule, askRule: state.askRule,
    payRuleFee: pay.feeCharged, askRuleFee: ask.feeCharged,
  });
}
const { payRule, askRule } = state;
const mandate = await view(net, funder, POLICY, 'get_mandate', [addr(wallet), u32(payRule)]);
step('mandate installed', { token: mandate.token, budget: String(mandate.budget), max_per_tx: String(mandate.max_per_tx), period_ledgers: mandate.period_ledgers });

// ---- agent signing via smart-account-kit (the Phase 7 path) ----
const agentSigner = Ed25519Signer.fromSecret(agent.secret(), net.ed25519Verifier);
const scheme = KNOWN_AUTH_DIGEST_SCHEMES[net.accountWasmHash];
const agentSign = (ruleIds) => async (entry, exp) =>
  signAuthEntryWithSigners(entry, [agentSigner], { networkPassphrase: net.passphrase, authDigestScheme: scheme, contextRuleIds: ruleIds, expiration: exp });

// x402 client side for a smart-account payer (what @fosay/sdk will ship)
async function x402Payload(amount) {
  const op = Operation.invokeContractFunction({ contract: token, function: 'transfer', args: [addr(wallet), addr(merchant.publicKey()), i128(amount)] });
  const mk = (o) => new TransactionBuilder(new Account(NULL_ACCOUNT, '0'), { fee: '100', networkPassphrase: net.passphrase }).addOperation(o).setTimeout(60).build();
  let tx = mk(op);
  let sim = await net.server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) return { error: diag(sim) };
  const auth = sim.result.auth;
  const exp = (await latest()) + PAYMENT_EXPIRY_LEDGERS;
  for (let i = 0; i < auth.length; i++) {
    const s = await agentSign([payRule])(auth[i], exp);
    auth[i] = typeof s === 'string' ? xdr.SorobanAuthorizationEntry.fromXDR(s, 'base64') : s;
  }
  const func = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
  tx = mk(Operation.invokeHostFunction({ func, auth }));
  sim = await net.server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) return { error: diag(sim) };
  return { xdr: rpc.assembleTransaction(tx, sim).build().toXDR(), minResourceFee: Number(sim.minResourceFee) };
}

const facilitator = new ExactStellarScheme([createEd25519Signer(facilitatorKp.secret(), `stellar:${net.name}`)]); // defaults
const reqs = (amount) => ({ scheme: 'exact', network: `stellar:${net.name}`, asset: token, payTo: merchant.publicKey(), amount: String(amount), maxTimeoutSeconds: MAX_TIMEOUT_SECONDS, extra: { areFeesSponsored: true } });
const remaining = async () => BigInt(await view(net, funder, POLICY, 'remaining_budget', [addr(wallet), u32(payRule)]));

// ---- 0. keeper: extend everything a payment touches ----
const KEEPER_EXTEND_TO = 31 * 17_280; // above OZ's 29-day threshold, so payments don't renew
const probePayment = await x402Payload(UNIT);
if (probePayment.error) throw new Error(`probe payment failed: ${json(probePayment.error)}`);
step('cold payment cost before keeper (not submitted)', { minResourceFee: probePayment.minResourceFee });
const kept = await extendFootprint(net, funder, probePayment.xdr, KEEPER_EXTEND_TO);
step('keeper extended footprint TTL to 31 days', kept);

// ---- A. x402 payments within the mandate (2 + 2 of a 5 budget, cap 2) ----
for (let i = 1; i <= 2; i++) {
  const amount = 2n * UNIT;
  const p = await x402Payload(amount);
  if (p.error) throw new Error(`payment ${i} client failed: ${json(p.error)}`);
  const payload = { x402Version: 2, accepted: reqs(amount), payload: { transaction: p.xdr } };
  const v = await facilitator.verify(payload, reqs(amount));
  if (!v.isValid) {
    step(`x402 payment ${i}: REJECTED by facilitator`, { minResourceFee: p.minResourceFee, verify: v });
    throw new Error('facilitator rejected an in-mandate payment');
  }
  const s = await facilitator.settle(payload, reqs(amount));
  if (!s.success) throw new Error(`settle ${i} failed: ${json(s)}`);
  const t = await net.server.getTransaction(s.transaction);
  step(`x402 payment ${i} (2 FUSD) verified + settled at default 50k ceiling`, {
    minResourceFee: p.minResourceFee, feeCharged: Number(t.resultXdr.feeCharged().toString()), tx: s.transaction, settled: s.success,
  });
}
step('remaining budget after payments', { remaining: String(await remaining()) });

// ---- B. refusals ----
const overCap = await x402Payload(3n * UNIT);
step('3 FUSD payment (over 2 FUSD per-tx cap) refused', { refused: !!overCap.error, contractErrors: overCap.error?.contractErrors });
const need = (await remaining()) + UNIT / 2n; // must be within the cap, over what's left
if (need > 2n * UNIT) throw new Error(`test setup: ${need} is over the per-tx cap; spend more first`);
const overBudget = await x402Payload(need);
step(`${Number(need) / 1e7} FUSD payment (over remaining budget) refused`, { refused: !!overBudget.error, contractErrors: overBudget.error?.contractErrors });

// ---- C. ask → owner approves ----
const memo = 'over budget: please approve';
const prop = await invoke(net, funder, {
  contract: APPROVALS, fn: 'propose',
  args: [addr(wallet), addr(token), addr(merchant.publicKey()), i128(need), bytes(mandateHash), str(memo), u32(1_000)],
  authSigner: agentSign([askRule]),
});
const id = BigInt(prop.result);
step('agent proposes (signed with smart-account-kit, :ask rule)', { intentId: String(id), feeCharged: prop.feeCharged });

const before = BigInt(await view(net, funder, token, 'balance', [addr(merchant.publicKey())]));
const exec = await invoke(net, funder, { contract: APPROVALS, fn: 'execute', args: [u64(id)], authSigner: ownerSign([0, 0]) });
const after = BigInt(await view(net, funder, token, 'balance', [addr(merchant.publicKey())]));
const intent = await view(net, funder, APPROVALS, 'get_intent', [u64(id)]);
step('owner approves execute(id) with rule ids [0, 0]', {
  status: intent.status, merchantReceived: String(after - before), feeCharged: exec.feeCharged, minResourceFee: exec.minResourceFee, tx: exec.hash,
});

const out = path.join(REPO, `deployments/${net.name}-smoke.json`);
fs.writeFileSync(out, json(report) + '\n');
console.log(`\nwrote ${path.relative(REPO, out)}`);
