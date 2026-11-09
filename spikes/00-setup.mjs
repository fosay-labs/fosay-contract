// Spike 0: verify canonical contracts, create test token, deploy a FOSAY-style wallet.
import { S, server, NET, ACCOUNT_WASM, ED25519_VERIFIER, SPENDING_LIMIT, state, save, key, fund, invoke, simulateOnly,
  addr, i128, u32, vec, smap, someU32, externalSigner, ctxCallContract, signSmartAccountEntry, j } from './common.mjs';
const { Operation, Asset, Address, TransactionBuilder, hash, xdr } = S;

const funder = key('funder'), issuer = key('issuer'), payee = key('payee'), facilitator = key('facilitator');
const owner = key('ownerKey'), agent = key('agentKey');

console.log('== 1. canonical contracts on testnet');
for (const [name, id] of [['ed25519 verifier', ED25519_VERIFIER], ['spending-limit v0.8.0', SPENDING_LIMIT]]) {
  const wasm = await server.getContractWasmByContractId(id);
  console.log(`  ${name} ${id}: wasm sha256 ${hash(wasm).toString('hex')}`);
}
const acctWasm = await server.getContractWasmByHash(Buffer.from(ACCOUNT_WASM, 'hex'));
console.log(`  account wasm ${ACCOUNT_WASM}: installed, ${acctWasm.length} bytes`);

console.log('== 2. fund accounts via friendbot');
for (const kp of [funder, issuer, payee, facilitator]) await fund(kp);
console.log('  funded', [funder, issuer, payee, facilitator].map((k) => k.publicKey().slice(0, 6)).join(' '));

async function classic(src, ops) {
  const acc = await server.getAccount(src.publicKey());
  let b = new TransactionBuilder(acc, { fee: '1000', networkPassphrase: NET });
  ops.forEach((o) => (b = b.addOperation(o)));
  const tx = b.setTimeout(120).build(); tx.sign(src);
  const r = await server.sendTransaction(tx);
  if (r.status === 'ERROR') throw new Error('classic tx error ' + r.errorResult?.toXDR('base64'));
  const p = await server.pollTransaction(r.hash, { attempts: 30 });
  if (p.status !== 'SUCCESS') throw new Error('classic tx ' + p.status);
}

console.log('== 3. test token FUSD (classic asset + SAC)');
const asset = new Asset('FUSD', issuer.publicKey());
const sac = asset.contractId(NET);
if (!state.sac) {
  try { await invoke(funder, null, null, null, { op: Operation.createStellarAssetContract({ asset }) }); } catch (e) { if (!String(e).includes('ExistingValue')) throw e; }
  await classic(payee, [Operation.changeTrust({ asset })]);
  state.sac = sac; save();
}
console.log('  SAC', sac);

console.log('== 4. deploy smart account (canonical OZ v0.8.0 wasm), owner = ed25519 External signer');
if (!state.account) {
  const r = await invoke(funder, null, null, null, {
    op: Operation.createCustomContract({
      address: new Address(funder.publicKey()),
      wasmHash: Buffer.from(ACCOUNT_WASM, 'hex'),
      salt: hash(Buffer.from('fosay-spike-' + owner.publicKey())),
      constructorArgs: [vec([externalSigner(ED25519_VERIFIER, owner)]), xdr.ScVal.scvMap([])],
    }),
  });
  state.account = r.result; save();
  console.log('  deploy fee (stroops):', r.fee, 'tx', r.txHash);
}
const account = state.account;
console.log('  account', account);
console.log('  rule 0:', j(await simulateOnly(funder, account, 'get_context_rule', [u32(0)])));

console.log('== 5. mint 1000 FUSD to the smart account');
if (!state.minted) {
  await invoke(issuer, sac, 'mint', [addr(account), i128(1000_0000000n)]);
  state.minted = true; save();
}
console.log('  balance:', String(await simulateOnly(funder, sac, 'balance', [addr(account)])));

console.log('== 6. add agent rule "agt1:pay": CallContract(FUSD SAC), agent signer, spending_limit 5 FUSD / 1000 ledgers');
if (state.agentRuleId == null) {
  const latest = (await server.getLatestLedger()).sequence;
  const params = smap([['period_ledgers', u32(1000)], ['spending_limit', i128(5_0000000n)]]);
  const policies = xdr.ScVal.scvMap([new xdr.ScMapEntry({ key: addr(SPENDING_LIMIT), val: params })]);
  const r = await invoke(funder, account, 'add_context_rule',
    [ctxCallContract(sac), S.nativeToScVal('agt1:pay', { type: 'string' }), someU32(latest + 100000), vec([externalSigner(ED25519_VERIFIER, agent)]), policies],
    { authSigner: (e, exp) => signSmartAccountEntry(e, { ruleIds: [0], signers: [owner], expLedger: exp }) });
  state.agentRuleId = r.result.id; save();
  console.log('  add_context_rule fee:', r.fee, 'tx', r.txHash);
}
console.log('  agent rule:', j(await simulateOnly(funder, account, 'get_context_rule', [u32(state.agentRuleId)])));
console.log('DONE');
