// Keeps the shared FOSAY contracts alive on testnet: extends the TTL of the FOSAY contract
// instances and code, the OZ verifiers and the canonical OZ smart account code whenever any of
// them has less than THRESHOLD_DAYS left. Per-wallet entries are the API keeper's job; this
// covers what every wallet depends on, even when no API is running.
//
//   node keep-alive.mjs            # throwaway Friendbot-funded key unless KEEPALIVE_SECRET is set
//
// Testnet only. Run weekly by .github/workflows/keep-alive.yml.
import fs from 'node:fs';
import path from 'node:path';

import { REPO, S, ensureFunded, network } from './lib.mjs';

const DAY = 17_280;
const THRESHOLD_DAYS = Number(process.env.KEEPALIVE_THRESHOLD_DAYS ?? 21);
const EXTEND_TO = 31 * DAY;

const net = network();
const dep = JSON.parse(fs.readFileSync(path.join(REPO, 'deployments', `${net.name}.json`), 'utf8'));
const instances = [
  dep.contracts.mandatePolicy.id,
  dep.contracts.approvals.id,
  dep.canonical.webauthnVerifier,
  dep.canonical.ed25519Verifier,
];
const latest = (await net.server.getLatestLedger()).sequence;

const instanceKeys = instances.map((id) => new S.Contract(id).getFootprint());
const found = (await net.server.getLedgerEntries(...instanceKeys)).entries;
const codeHashes = new Set([dep.canonical.accountWasmHash]);
for (const e of found) {
  const exe = e.val.contractData().val().instance().executable();
  if (exe.switch().name === 'contractExecutableWasm') codeHashes.add(exe.wasmHash().toString('hex'));
}
const missing = instances.filter((_, i) => !found.some((e) => e.key.toXDR('base64') === instanceKeys[i].toXDR('base64')));
if (missing.length) throw new Error(`archived or missing (restore or redeploy first): ${missing.join(', ')}`);

const codeKeys = [...codeHashes].map((h) =>
  S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({ hash: Buffer.from(h, 'hex') })),
);
const all = [...found, ...(await net.server.getLedgerEntries(...codeKeys)).entries];
const due = all.filter((e) => e.liveUntilLedgerSeq - latest < THRESHOLD_DAYS * DAY);
for (const e of all) {
  const kind = e.key.switch().name === 'contractCode' ? 'code' : 'instance';
  console.log(`${kind.padEnd(8)} ${((e.liveUntilLedgerSeq - latest) / DAY).toFixed(1).padStart(6)} days left`);
}
if (due.length === 0) {
  console.log(`nothing under ${THRESHOLD_DAYS} days`);
  process.exit(0);
}

const source = process.env.KEEPALIVE_SECRET ? S.Keypair.fromSecret(process.env.KEEPALIVE_SECRET) : S.Keypair.random();
await ensureFunded(net, source);
const account = await net.server.getAccount(source.publicKey());
const tx = new S.TransactionBuilder(account, { fee: '1000000', networkPassphrase: net.passphrase })
  .setSorobanData(new S.SorobanDataBuilder().setReadOnly(due.map((e) => e.key)).build())
  .addOperation(S.Operation.extendFootprintTtl({ extendTo: EXTEND_TO }))
  .setTimeout(120)
  .build();
const prepared = await net.server.prepareTransaction(tx);
prepared.sign(source);
const sent = await net.server.sendTransaction(prepared);
if (sent.status === 'ERROR') throw new Error('extendFootprintTtl send failed');
const res = await net.server.pollTransaction(sent.hash, { attempts: 40 });
if (res.status !== 'SUCCESS') throw new Error(`extendFootprintTtl ${res.status} (${sent.hash})`);
console.log(`extended ${due.length} entries to 31 days: ${sent.hash}`);
