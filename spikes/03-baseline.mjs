// S4 baseline: the same x402 payment from a plain G-account using the STOCK x402 client.
import { state, key, server } from './common.mjs';
import { ExactStellarScheme as ClientScheme } from '@x402/stellar/exact/client';
import { ExactStellarScheme as FacScheme } from '@x402/stellar/exact/facilitator';
import { createEd25519Signer } from '@x402/stellar';

const payee = key('payee'), issuer = key('issuer'), facilitatorKp = key('facilitator');
// payee (G, holds FUSD) pays the smart account 0.1 FUSD, stock client end to end
const r = { scheme: 'exact', network: 'stellar:testnet', asset: state.sac, payTo: state.account, amount: '1000000', maxTimeoutSeconds: 60, extra: { areFeesSponsored: true } };
const client = new ClientScheme(createEd25519Signer(payee.secret(), 'stellar:testnet'));
const pp = await client.createPaymentPayload(2, r);
const payload = { x402Version: 2, accepted: r, payload: pp.payload };
const f = new FacScheme([createEd25519Signer(facilitatorKp.secret(), 'stellar:testnet')]);
const v = await f.verify(payload, r);
console.log('G-account verify (default ceiling):', JSON.stringify(v));
if (v.isValid) {
  const s = await f.settle(payload, r);
  const t = await server.getTransaction(s.transaction);
  console.log('settled', s.success, 'fee charged (stroops):', t.resultXdr?.feeCharged().toString());
}
