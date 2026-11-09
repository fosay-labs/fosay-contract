// Shared helpers for FOSAY deploy and smoke scripts (Node 22, @stellar/stellar-sdk 16.4.1).
import fs from 'node:fs';
import path from 'node:path';
import * as S from '@stellar/stellar-sdk';

const { rpc, Keypair, Networks, TransactionBuilder, Operation, xdr, Address, hash } = S;
export { S };

export const NETWORKS = {
  testnet: {
    name: 'testnet',
    rpcUrl: process.env.STELLAR_RPC_URL ?? 'https://soroban-testnet.stellar.org',
    passphrase: Networks.TESTNET,
    friendbot: 'https://friendbot.stellar.org',
    // canonical OZ stellar-accounts v0.8.0 set (smart-account-kit deployment manifest)
    accountWasmHash: 'c514d35ea67eaea9bfc1fb3327854c0c06111bc1ddebd4fe217c504c1004b9e1',
    webauthnVerifier: 'CC7EKIHQP3TN4CARQDND6CEOY2UXLWWC2X5GHTD5NLAT7BG5GPZIOM3F',
    ed25519Verifier: 'CAAVTMCBXEIBPR64EAASKFXERVPYFZA2JYP5A3BG6PESWEFUJX5IHKN4',
  },
};

export const REPO = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
export const WASM_DIR = path.join(REPO, 'target/wasm32v1-none/release');
export const SECRETS_DIR = path.join(REPO, 'scripts/.secrets');

export function network() {
  const name = process.env.STELLAR_NETWORK ?? 'testnet';
  const n = NETWORKS[name];
  if (!n) throw new Error(`unsupported STELLAR_NETWORK=${name} (only testnet is configured)`);
  return { ...n, server: new rpc.Server(n.rpcUrl) };
}

/** Loads a key from env, else from scripts/.secrets/<name>, else creates one (testnet only). */
export function loadKey(name, envVar) {
  if (envVar && process.env[envVar]) return Keypair.fromSecret(process.env[envVar]);
  const file = path.join(SECRETS_DIR, name);
  if (fs.existsSync(file)) return Keypair.fromSecret(fs.readFileSync(file, 'utf8').trim());
  fs.mkdirSync(SECRETS_DIR, { recursive: true, mode: 0o700 });
  const kp = Keypair.random();
  fs.writeFileSync(file, kp.secret(), { mode: 0o600 });
  return kp;
}

export async function ensureFunded(net, kp) {
  try {
    await net.server.getAccount(kp.publicKey());
    return false;
  } catch {
    if (!net.friendbot) throw new Error(`${kp.publicKey()} is not funded`);
    const r = await fetch(`${net.friendbot}?addr=${kp.publicKey()}`);
    if (!r.ok) throw new Error(`friendbot ${r.status}: ${await r.text()}`);
    return true;
  }
}

// ---- ScVal helpers ----
export const sym = (s) => xdr.ScVal.scvSymbol(s);
export const u32 = (n) => xdr.ScVal.scvU32(n);
export const u64 = (n) => S.nativeToScVal(BigInt(n), { type: 'u64' });
export const i128 = (n) => S.nativeToScVal(BigInt(n), { type: 'i128' });
export const addr = (a) => new Address(a).toScVal();
export const bytes = (b) => xdr.ScVal.scvBytes(Buffer.from(b));
export const str = (s) => S.nativeToScVal(s, { type: 'string' });
export const vec = (a) => xdr.ScVal.scvVec(a);
export const smap = (pairs) =>
  xdr.ScVal.scvMap(pairs.map(([k, v]) => new xdr.ScMapEntry({ key: typeof k === 'string' ? sym(k) : k, val: v })));
export const someU32 = (n) => (n == null ? xdr.ScVal.scvVoid() : u32(n));
export const externalSigner = (verifier, kp) => vec([sym('External'), addr(verifier), bytes(kp.rawPublicKey())]);
export const ctxCallContract = (c) => vec([sym('CallContract'), addr(c)]);

/** Owner-side signing for the OZ v0.8.0 account (preimage-v1), any invocation incl. admin ops. */
export function signOwnerEntry(net, entry, { ruleIds, signers, expLedger, verifier }) {
  const creds = entry.credentials().address();
  creds.signatureExpirationLedger(expLedger);
  const pre = xdr.HashIdPreimage.envelopeTypeSorobanAuthorization(
    new xdr.HashIdPreimageSorobanAuthorization({
      networkId: hash(Buffer.from(net.passphrase)),
      nonce: creds.nonce(),
      signatureExpirationLedger: expLedger,
      invocation: entry.rootInvocation(),
    }),
  );
  const account = Address.fromScAddress(creds.address()).toString();
  const digest = hash(
    smap([
      ['account', addr(account)],
      ['context_rule_ids', vec(ruleIds.map(u32))],
      ['signature_payload', bytes(hash(pre.toXDR()))],
    ]).toXDR(),
  );
  const sorted = [...signers].sort((a, b) => Buffer.compare(a.rawPublicKey(), b.rawPublicKey()));
  const sigs = xdr.ScVal.scvMap(
    sorted.map((kp) => new xdr.ScMapEntry({ key: externalSigner(verifier, kp), val: bytes(kp.sign(digest)) })),
  );
  creds.signature(smap([['context_rule_ids', vec(ruleIds.map(u32))], ['signers', sigs]]));
  return entry;
}

export function diag(sim) {
  const codes = [];
  for (const d of sim.events ?? []) {
    const m = JSON.stringify(d).match(/Error\(Contract, #(\d+)\)/);
    if (m) codes.push(Number(m[1]));
  }
  const txt = String(sim.error ?? '');
  const c = txt.match(/Error\(Contract, #(\d+)\)/g) ?? [];
  return { error: txt.split('\n')[0], contractErrors: [...new Set([...codes, ...c.map((x) => Number(x.match(/\d+/)[0]))])] };
}

/**
 * Invoke a contract (or a raw host-function op). `authSigner(entry, expLedger)` signs every
 * address-credential entry in place (may be async). Returns result, hash, fee charged and
 * the simulation's min resource fee.
 */
export async function invoke(net, source, { contract, fn, args, op, authSigner }) {
  const server = net.server;
  const operation = op ?? Operation.invokeContractFunction({ contract, function: fn, args });
  const build = async (o) =>
    new TransactionBuilder(await server.getAccount(source.publicKey()), { fee: '10000000', networkPassphrase: net.passphrase })
      .addOperation(o)
      .setTimeout(180)
      .build();
  let tx = await build(operation);
  let sim = await server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) {
    const d = diag(sim);
    throw Object.assign(new Error(`simulate ${fn ?? 'op'}: ${d.error}`), d);
  }
  const auth = sim.result?.auth ?? [];
  if (auth.some((e) => e.credentials().switch().name === 'sorobanCredentialsAddress')) {
    if (!authSigner) throw new Error(`${fn}: needs address auth but no signer given`);
    const exp = (await server.getLatestLedger()).sequence + 60;
    for (let i = 0; i < auth.length; i++) {
      if (auth[i].credentials().switch().name === 'sorobanCredentialsAddress') {
        const signed = await authSigner(auth[i], exp);
        if (signed) auth[i] = typeof signed === 'string' ? xdr.SorobanAuthorizationEntry.fromXDR(signed, 'base64') : signed;
      }
    }
    const func = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
    tx = await build(Operation.invokeHostFunction({ func, auth }));
    sim = await server.simulateTransaction(tx);
    if (!rpc.Api.isSimulationSuccess(sim)) {
      const d = diag(sim);
      throw Object.assign(new Error(`re-simulate ${fn ?? 'op'} with signed auth: ${d.error}`), d);
    }
  }
  const prepared = rpc.assembleTransaction(tx, sim).build();
  prepared.sign(source);
  const sent = await server.sendTransaction(prepared);
  if (sent.status === 'ERROR') throw new Error(`send ${fn ?? 'op'}: ${sent.errorResult?.toXDR('base64')}`);
  const res = await server.pollTransaction(sent.hash, { attempts: 40 });
  if (res.status !== 'SUCCESS') throw new Error(`tx ${sent.hash} ${res.status}`);
  return {
    result: res.returnValue ? S.scValToNative(res.returnValue) : undefined,
    hash: sent.hash,
    ledger: res.ledger,
    feeCharged: Number(res.resultXdr.feeCharged().toString()),
    minResourceFee: Number(sim.minResourceFee),
  };
}

export async function view(net, source, contract, fn, args = []) {
  const tx = new TransactionBuilder(await net.server.getAccount(source.publicKey()), { fee: '100', networkPassphrase: net.passphrase })
    .addOperation(Operation.invokeContractFunction({ contract, function: fn, args }))
    .setTimeout(60)
    .build();
  const sim = await net.server.simulateTransaction(tx);
  if (!rpc.Api.isSimulationSuccess(sim)) {
    const d = diag(sim);
    throw Object.assign(new Error(`view ${fn}: ${d.error}`), d);
  }
  return S.scValToNative(sim.result.retval);
}

export const json = (x) =>
  JSON.stringify(x, (_, v) => (typeof v === 'bigint' ? v.toString() : Buffer.isBuffer(v) ? v.toString('hex') : v), 2);

/**
 * Keeper primitive: extend the TTL of every persistent entry and contract code a
 * transaction touches (its Soroban footprint) to `extendToLedgers`, in one
 * ExtendFootprintTTL operation. Temporary entries (nonces) and classic entries are skipped.
 */
export async function extendFootprint(net, source, assembledTxXdr, extendToLedgers) {
  const tx = new S.Transaction(assembledTxXdr, net.passphrase);
  const fp = tx.toEnvelope().v1().tx().ext().sorobanData().resources().footprint();
  const keys = [...fp.readOnly(), ...fp.readWrite()].filter((k) => {
    const t = k.switch().name;
    if (t === 'contractCode') return true;
    return t === 'contractData' && k.contractData().durability().name === 'persistent';
  });
  const account = await net.server.getAccount(source.publicKey());
  const ext = new S.TransactionBuilder(account, { fee: '1000000', networkPassphrase: net.passphrase })
    .setSorobanData(new S.SorobanDataBuilder().setReadOnly(keys).build())
    .addOperation(S.Operation.extendFootprintTtl({ extendTo: extendToLedgers }))
    .setTimeout(120)
    .build();
  const prepared = await net.server.prepareTransaction(ext);
  prepared.sign(source);
  const sent = await net.server.sendTransaction(prepared);
  if (sent.status === 'ERROR') throw new Error('extendFootprintTtl send failed');
  const res = await net.server.pollTransaction(sent.hash, { attempts: 40 });
  if (res.status !== 'SUCCESS') throw new Error(`extendFootprintTtl ${res.status}`);
  return { hash: sent.hash, entries: keys.length, feeCharged: Number(res.resultXdr.feeCharged().toString()) };
}
