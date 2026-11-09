// Shared helpers for FOSAY testnet spikes.
import * as S from '@stellar/stellar-sdk';
import fs from 'node:fs';

const { rpc, Keypair, Networks, TransactionBuilder, Operation, Asset, xdr, Address, hash, StrKey } = S;
export { S };

export const RPC_URL = 'https://soroban-testnet.stellar.org';
export const server = new rpc.Server(RPC_URL);
export const NET = Networks.TESTNET;

// Canonical deployments (smart-account-kit docs, verified 2026-11-09)
export const ACCOUNT_WASM = 'c514d35ea67eaea9bfc1fb3327854c0c06111bc1ddebd4fe217c504c1004b9e1';
export const ED25519_VERIFIER = 'CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4';
export const SPENDING_LIMIT = 'CC3CEYQAP6NPNFIYSWL5MVZRCPJZGWX4XO7WDEJOUVGO2FZX5OONYPA5';

const STATE = new URL('./state.json', import.meta.url);
export const state = fs.existsSync(STATE) ? JSON.parse(fs.readFileSync(STATE)) : {};
export const save = () => fs.writeFileSync(STATE, JSON.stringify(state, null, 2));

export function key(name) {
  if (!state[name]) { state[name] = Keypair.random().secret(); save(); }
  return Keypair.fromSecret(state[name]);
}

export async function fund(kp) {
  try { await server.getAccount(kp.publicKey()); return; } catch {}
  const r = await fetch(`https://friendbot.stellar.org?addr=${kp.publicKey()}`);
  if (!r.ok) throw new Error(`friendbot ${r.status}: ${await r.text()}`);
}

// ---- ScVal helpers ----
export const sym = (s) => xdr.ScVal.scvSymbol(s);
export const u32 = (n) => xdr.ScVal.scvU32(n);
export const i128 = (n) => S.nativeToScVal(BigInt(n), { type: 'i128' });
export const addr = (a) => new Address(a).toScVal();
export const bytes = (b) => xdr.ScVal.scvBytes(Buffer.from(b));
export const vec = (a) => xdr.ScVal.scvVec(a);
export const smap = (pairs) => xdr.ScVal.scvMap(pairs.map(([k, v]) => new xdr.ScMapEntry({ key: typeof k === 'string' ? sym(k) : k, val: v })));
export const someU32 = (n) => (n == null ? xdr.ScVal.scvVoid() : u32(n));

// Signer::External(verifier, pubkey)
export const externalSigner = (verifier, kp) => vec([sym('External'), addr(verifier), bytes(kp.rawPublicKey())]);
// ContextRuleType
export const ctxDefault = () => vec([sym('Default')]);
export const ctxCallContract = (c) => vec([sym('CallContract'), addr(c)]);

// ---- Smart-account auth signing (OZ stellar-accounts 0.8.0, preimage-v1) ----
export function signSmartAccountEntry(entry, { ruleIds, signers, expLedger, verifier = ED25519_VERIFIER }) {
  const creds = entry.credentials().address();
  creds.signatureExpirationLedger(expLedger);
  const pre = xdr.HashIdPreimage.envelopeTypeSorobanAuthorization(
    new xdr.HashIdPreimageSorobanAuthorization({
      networkId: hash(Buffer.from(NET)),
      nonce: creds.nonce(),
      signatureExpirationLedger: expLedger,
      invocation: entry.rootInvocation(),
    }),
  );
  const signaturePayload = hash(pre.toXDR());
  const account = Address.fromScAddress(creds.address()).toString();
  const digestPreimage = smap([
    ['account', addr(account)],
    ['context_rule_ids', vec(ruleIds.map(u32))],
    ['signature_payload', bytes(signaturePayload)],
  ]);
  const digest = hash(digestPreimage.toXDR());
  const sorted = [...signers].sort((a, b) => Buffer.compare(a.rawPublicKey(), b.rawPublicKey()));
  const sigMap = xdr.ScVal.scvMap(sorted.map((kp) => new xdr.ScMapEntry({ key: externalSigner(verifier, kp), val: bytes(kp.sign(digest)) })));
  creds.signature(smap([['context_rule_ids', vec(ruleIds.map(u32))], ['signers', sigMap]]));
  return { entry, digest, signaturePayload, digestPreimage };
}

// ---- Transaction flow ----
export function diag(sim) {
  const ev = (sim.events || []).map((e) => {
    try { return JSON.stringify(S.scValToNative(e.event().body().v0().data()), (_, v) => (typeof v === 'bigint' ? v.toString() : v)); } catch { return '?'; }
  });
  return `${sim.error}\n  last events: ${ev.slice(-6).join('\n   ')}`;
}

/**
 * Invoke a contract. `authSigner(entry)` is called for every auth entry whose
 * credentials are an Address other than the tx source; it must sign in place.
 * Returns { result, txHash, fee, sim }.
 */
export async function invoke(source, contractId, method, args, { authSigner, op, recordMode } = {}) {
  const account = await server.getAccount(source.publicKey());
  const operation = op ?? Operation.invokeContractFunction({ contract: contractId, function: method, args });
  let tx = new TransactionBuilder(account, { fee: '10000000', networkPassphrase: NET }).addOperation(operation).setTimeout(180).build();
  let sim = await server.simulateTransaction(tx, undefined, recordMode);
  if (!rpc.Api.isSimulationSuccess(sim)) throw new Error(`simulate(${method}) failed: ${diag(sim)}`);

  const auth = sim.result?.auth ?? [];
  const needsSigning = auth.some((e) => e.credentials().switch().name === 'sorobanCredentialsAddress');
  if (needsSigning) {
    if (!authSigner) throw new Error(`${method}: auth entries need signing but no authSigner given`);
    const latest = (await server.getLatestLedger()).sequence;
    for (const e of auth) if (e.credentials().switch().name === 'sorobanCredentialsAddress') await authSigner(e, latest + 60);
    const func = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
    const account2 = await server.getAccount(source.publicKey());
    tx = new TransactionBuilder(account2, { fee: '10000000', networkPassphrase: NET })
      .addOperation(Operation.invokeHostFunction({ func, auth })).setTimeout(180).build();
    sim = await server.simulateTransaction(tx);
    if (!rpc.Api.isSimulationSuccess(sim)) throw new Error(`re-simulate(${method}) with signed auth failed: ${diag(sim)}`);
  }
  const prepared = rpc.assembleTransaction(tx, sim).build();
  prepared.sign(source);
  const sent = await server.sendTransaction(prepared);
  if (sent.status === 'ERROR') throw new Error(`send(${method}) error: ${sent.errorResult?.toXDR('base64')}`);
  const res = await server.pollTransaction(sent.hash, { attempts: 30 });
  if (res.status !== 'SUCCESS') throw new Error(`tx ${sent.hash} ${res.status}`);
  const fee = res.resultXdr ? Number(res.resultXdr.feeCharged().toString()) : undefined;
  return { result: res.returnValue ? S.scValToNative(res.returnValue) : undefined, txHash: sent.hash, fee, minResourceFee: sim.minResourceFee };
}

export async function simulateOnly(source, contractId, method, args) {
  const account = await server.getAccount(source.publicKey());
  const tx = new TransactionBuilder(account, { fee: '100', networkPassphrase: NET })
    .addOperation(Operation.invokeContractFunction({ contract: contractId, function: method, args })).setTimeout(60).build();
  const sim = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) throw new Error(`simulate(${method}) failed: ${diag(sim)}`);
  return S.scValToNative(sim.result.retval);
}

export const j = (x) => JSON.stringify(x, (_, v) => (typeof v === 'bigint' ? v.toString() : Buffer.isBuffer(v) ? v.toString('hex') : v), 2);
