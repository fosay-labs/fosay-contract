// Deploy fosay-mandate-policy and fosay-approvals.
//
//   stellar contract build
//   cd scripts && npm install && node deploy.mjs
//
// Env: STELLAR_NETWORK (testnet), STELLAR_RPC_URL, STELLAR_DEPLOYER_SECRET (optional on
// testnet: a key is created in scripts/.secrets/deployer and funded by Friendbot).
// Idempotent: the salt is derived from the wasm hash, so re-running with the same build
// reuses the existing contracts. Writes deployments/<network>.json.
import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';
import { S, network, loadKey, ensureFunded, invoke, view, u32, addr, json, REPO, WASM_DIR } from './lib.mjs';

const { Operation, Address, hash, xdr, StrKey } = S;

const APPROVALS_MAX_PENDING = 20;
const APPROVALS_MAX_TTL_LEDGERS = 120_960; // ~7 days

const CONTRACTS = [
  { key: 'mandatePolicy', crate: 'fosay-mandate-policy', file: 'fosay_mandate_policy.wasm', ctor: [] },
  {
    key: 'approvals',
    crate: 'fosay-approvals',
    file: 'fosay_approvals.wasm',
    ctor: [u32(APPROVALS_MAX_PENDING), u32(APPROVALS_MAX_TTL_LEDGERS)],
  },
];

function contractIdFor(net, deployer, salt) {
  const preimage = xdr.HashIdPreimage.envelopeTypeContractId(
    new xdr.HashIdPreimageContractId({
      networkId: hash(Buffer.from(net.passphrase)),
      contractIdPreimage: xdr.ContractIdPreimage.contractIdPreimageFromAddress(
        new xdr.ContractIdPreimageFromAddress({ address: new Address(deployer).toScAddress(), salt }),
      ),
    }),
  );
  return StrKey.encodeContract(hash(preimage.toXDR()));
}

async function exists(fn) {
  try {
    await fn();
    return true;
  } catch {
    return false;
  }
}

const net = network();
const deployer = loadKey('deployer', 'STELLAR_DEPLOYER_SECRET');
const commit = execSync('git rev-parse --short HEAD', { cwd: REPO }).toString().trim();
const dirty = execSync('git status --porcelain -- contracts Cargo.toml Cargo.lock', { cwd: REPO }).toString().trim() !== '';
console.log(`network   ${net.name} (${net.rpcUrl})`);
console.log(`deployer  ${deployer.publicKey()}`);
console.log(`commit    ${commit}${dirty ? ' (contracts have uncommitted changes!)' : ''}`);
if (await ensureFunded(net, deployer)) console.log('          funded via Friendbot');

const out = {
  network: net.name,
  rpcUrl: net.rpcUrl,
  networkPassphrase: net.passphrase,
  deployedAt: new Date().toISOString(),
  sourceCommit: commit,
  deployer: deployer.publicKey(),
  contracts: {},
  canonical: {
    accountWasmHash: net.accountWasmHash,
    webauthnVerifier: net.webauthnVerifier,
    ed25519Verifier: net.ed25519Verifier,
  },
};

for (const c of CONTRACTS) {
  const file = path.join(WASM_DIR, c.file);
  if (!fs.existsSync(file)) throw new Error(`${file} missing: run \`stellar contract build\` first`);
  const wasm = fs.readFileSync(file);
  const wasmHash = hash(wasm);
  console.log(`\n${c.crate}: ${wasm.length} bytes, wasm hash ${wasmHash.toString('hex')}`);

  let uploadTx = null;
  if (await exists(() => net.server.getContractWasmByHash(wasmHash))) {
    console.log('  wasm already installed');
  } else {
    const r = await invoke(net, deployer, { op: Operation.uploadContractWasm({ wasm }) });
    uploadTx = r.hash;
    console.log(`  uploaded  tx ${r.hash} fee ${r.feeCharged}`);
  }

  const salt = hash(Buffer.from(`fosay:${c.crate}:${wasmHash.toString('hex')}`));
  const id = contractIdFor(net, deployer.publicKey(), salt);
  let deployTx = null;
  if (await exists(() => net.server.getContractWasmByContractId(id))) {
    console.log(`  contract already deployed ${id}`);
  } else {
    const r = await invoke(net, deployer, {
      op: Operation.createCustomContract({
        address: new Address(deployer.publicKey()),
        wasmHash,
        salt,
        constructorArgs: c.ctor,
      }),
    });
    if (r.result !== id) throw new Error(`deployed id ${r.result} != predicted ${id}`);
    deployTx = r.hash;
    console.log(`  deployed  ${id} tx ${r.hash} ledger ${r.ledger} fee ${r.feeCharged}`);
  }
  out.contracts[c.key] = { crate: c.crate, id, wasmHash: wasmHash.toString('hex'), uploadTx, deployTx };
}

// sanity checks against the live contracts
const probe = deployer.publicKey();
const pending = await view(net, deployer, out.contracts.approvals.id, 'pending_count', [addr(probe)]);
if (pending !== 0) throw new Error(`unexpected pending_count ${pending}`);
let notInstalled = false;
try {
  await view(net, deployer, out.contracts.mandatePolicy.id, 'get_mandate', [addr(probe), u32(0)]);
} catch (e) {
  notInstalled = (e.contractErrors ?? []).includes(4100) || /#4100/.test(String(e));
}
if (!notInstalled) throw new Error('get_mandate on an empty account did not return NotInstalled (4100)');
console.log('\nsanity: approvals.pending_count = 0, mandate_policy.get_mandate → NotInstalled (4100) ✓');

const prev = path.join(REPO, `deployments/${net.name}.json`);
if (fs.existsSync(prev)) {
  const old = JSON.parse(fs.readFileSync(prev, 'utf8'));
  for (const k of Object.keys(out.contracts)) {
    if (old.contracts?.[k]?.id === out.contracts[k].id) {
      out.contracts[k].uploadTx ??= old.contracts[k].uploadTx;
      out.contracts[k].deployTx ??= old.contracts[k].deployTx;
    }
  }
}
fs.mkdirSync(path.dirname(prev), { recursive: true });
fs.writeFileSync(prev, json(out) + '\n');
console.log(`\nwrote deployments/${net.name}.json\n`);
console.log('# ---- copy into fosay-app .env ----');
console.log(`STELLAR_NETWORK=${net.name}`);
console.log(`STELLAR_RPC_URL=${net.rpcUrl}`);
console.log(`STELLAR_NETWORK_PASSPHRASE="${net.passphrase}"`);
console.log(`FOSAY_MANDATE_POLICY_ID=${out.contracts.mandatePolicy.id}`);
console.log(`FOSAY_APPROVALS_ID=${out.contracts.approvals.id}`);
console.log(`OZ_ACCOUNT_WASM_HASH=${net.accountWasmHash}`);
console.log(`OZ_WEBAUTHN_VERIFIER_ID=${net.webauthnVerifier}`);
console.log(`OZ_ED25519_VERIFIER_ID=${net.ed25519Verifier}`);
