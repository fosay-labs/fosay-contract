//! Test harness: the canonical OpenZeppelin smart account (v0.8.0 wasm fixture), the
//! canonical Ed25519 verifier, a Stellar Asset Contract, and helpers that build
//! *real* signed smart-account auth entries. Modeled on `stellar-accounts` 0.8.0
//! `src/smart_account/test/auth_entries.rs`.
#![allow(dead_code)]

extern crate std;

use core::cell::Cell;

use ed25519_dalek::{Signer as _, SigningKey};
use soroban_sdk::{
    map,
    testutils::{Address as _, Ledger},
    token, vec,
    xdr::{
        Hash as XdrHash, HashIdPreimage, HashIdPreimageSorobanAuthorization, InvokeContractArgs,
        Limits, ScVal, SorobanAddressCredentials, SorobanAuthorizationEntry,
        SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, VecM, WriteXdr,
    },
    Address, Bytes, BytesN, Env, IntoVal, Map, String, TryFromVal, Val, Vec,
};
use stellar_accounts::smart_account::{AuthDigestPreimage, AuthPayload, Signer};

use crate::{contract::MandatePolicy, types::MandateParams};

pub mod oz_account {
    soroban_sdk::contractimport!(file = "../../tests/fixtures/oz_smart_account_v0_8_0.wasm");
}
pub mod oz_ed25519 {
    soroban_sdk::contractimport!(file = "../../tests/fixtures/oz_ed25519_verifier.wasm");
}

pub const OWNER_SEED: [u8; 32] = [11; 32];
pub const AGENT_SEED: [u8; 32] = [22; 32];
pub const STRANGER_SEED: [u8; 32] = [33; 32];

/// 1 token = 10^7 base units (USDC-style 7 decimals).
pub const UNIT: i128 = 10_000_000;
pub const START_LEDGER: u32 = 1_000;
pub const OWNER_RULE: u32 = 0;

pub struct Wallet {
    pub e: Env,
    pub account: Address,
    pub verifier: Address,
    pub policy: Address,
    pub token: Address,
    pub owner: SigningKey,
    pub agent: SigningKey,
    pub stranger: SigningKey,
    pub pay_rule: u32,
    nonce: Cell<i64>,
}

pub fn key(seed: [u8; 32]) -> SigningKey {
    SigningKey::from_bytes(&seed)
}

/// `Signer::External(verifier, pubkey)` as the stellar-accounts type.
pub fn signer(e: &Env, verifier: &Address, k: &SigningKey) -> Signer {
    Signer::External(
        verifier.clone(),
        Bytes::from_array(e, k.verifying_key().as_bytes()),
    )
}

/// The same signer as the imported account contract's type.
pub fn oz_signer(e: &Env, verifier: &Address, k: &SigningKey) -> oz_account::Signer {
    oz_account::Signer::External(
        verifier.clone(),
        Bytes::from_array(e, k.verifying_key().as_bytes()),
    )
}

pub fn params(e: &Env, budget: i128, period: u32, max_per_tx: i128) -> MandateParams {
    MandateParams {
        budget,
        period_ledgers: period,
        max_per_tx,
        recipients: Vec::new(e),
        mandate_hash: BytesN::from_array(e, &[7; 32]),
    }
}

impl Wallet {
    /// Deploys the canonical account with the owner key on rule 0, funds it with
    /// 1,000 tokens, and adds the agent `:pay` rule with this policy installed.
    pub fn new(budget: i128, period: u32, max_per_tx: i128) -> Wallet {
        Wallet::with_params(|e| params(e, budget, period, max_per_tx))
    }

    /// Like [`Wallet::new`] with params built inside the wallet's `Env`.
    pub fn with_params(make: impl FnOnce(&Env) -> MandateParams) -> Wallet {
        let e = Env::default();
        e.cost_estimate().budget().reset_unlimited();
        e.ledger().set_sequence_number(START_LEDGER);
        let verifier = e.register(oz_ed25519::WASM, ());
        let policy = e.register(MandatePolicy, ());
        let owner = key(OWNER_SEED);
        let agent = key(AGENT_SEED);
        let stranger = key(STRANGER_SEED);
        let account = e.register(
            oz_account::WASM,
            (
                vec![&e, oz_signer(&e, &verifier, &owner)],
                Map::<Address, Val>::new(&e),
            ),
        );
        let admin = Address::generate(&e);
        let token = e.register_stellar_asset_contract_v2(admin).address();

        // Setup runs with mocked auth; tests switch to real signed entries afterwards.
        e.mock_all_auths();
        token::StellarAssetClient::new(&e, &token).mint(&account, &(1_000 * UNIT));
        let w = Wallet {
            e,
            account,
            verifier,
            policy,
            token,
            owner,
            agent,
            stranger,
            pay_rule: 0,
            nonce: Cell::new(1),
        };
        let p = make(&w.e);
        let pay_rule = w.add_agent_rule(p);
        w.e.set_auths(&[]);
        Wallet { pay_rule, ..w }
    }

    pub fn account_client(&self) -> oz_account::Client<'_> {
        oz_account::Client::new(&self.e, &self.account)
    }

    pub fn policy_client(&self) -> crate::contract::MandatePolicyClient<'_> {
        crate::contract::MandatePolicyClient::new(&self.e, &self.policy)
    }

    pub fn token_client(&self) -> token::Client<'_> {
        token::Client::new(&self.e, &self.token)
    }

    /// Adds `CallContract(token)` rule "agt1:pay" with the agent signer and this policy.
    /// Must run under `mock_all_auths` (setup) or with an owner-signed entry.
    pub fn add_agent_rule(&self, p: MandateParams) -> u32 {
        let mut policies: Map<Address, Val> = Map::new(&self.e);
        policies.set(self.policy.clone(), p.into_val(&self.e));
        self.account_client()
            .add_context_rule(
                &oz_account::ContextRuleType::CallContract(self.token.clone()),
                &String::from_str(&self.e, "agt1:pay"),
                &None,
                &vec![&self.e, oz_signer(&self.e, &self.verifier, &self.agent)],
                &policies,
            )
            .id
    }

    pub fn balance(&self, who: &Address) -> i128 {
        self.token_client().balance(who)
    }

    pub fn advance(&self, ledgers: u32) {
        let s = self.e.ledger().sequence();
        self.e.ledger().set_sequence_number(s + ledgers);
    }

    fn next_nonce(&self) -> i64 {
        let n = self.nonce.get();
        self.nonce.set(n + 1);
        n
    }

    /// A signed address-credentials entry for this account over `invocation`.
    pub fn signed_entry(
        &self,
        keys: &[&SigningKey],
        rule_ids: &[u32],
        invocation: SorobanAuthorizedInvocation,
    ) -> SorobanAuthorizationEntry {
        let e = &self.e;
        let nonce = self.next_nonce();
        let expiration = e.ledger().sequence() + 100;
        let payload = signature_payload(e, nonce, expiration, &invocation);
        let ids = Vec::from_slice(e, rule_ids);
        let digest = AuthDigestPreimage {
            account: self.account.clone(),
            signature_payload: payload,
            context_rule_ids: ids.clone(),
        }
        .digest(e);
        let mut signers: Map<Signer, Bytes> = map![e];
        for k in keys {
            let sig = k.sign(&digest.to_array()).to_bytes();
            signers.set(signer(e, &self.verifier, k), Bytes::from_array(e, &sig));
        }
        let auth = AuthPayload {
            signers,
            context_rule_ids: ids,
        };
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: (&self.account).into(),
                nonce,
                signature_expiration_ledger: expiration,
                signature: to_scval(e, auth),
            }),
            root_invocation: invocation,
        }
    }

    /// `token.transfer(account, to, amount)` as an authorized invocation.
    pub fn transfer_invocation(&self, to: &Address, amount: i128) -> SorobanAuthorizedInvocation {
        contract_fn(
            &self.token,
            "transfer",
            std::vec![
                to_scval(&self.e, self.account.clone()),
                to_scval(&self.e, to.clone()),
                to_scval(&self.e, amount),
            ],
        )
    }

    /// Attempts `transfer(account -> to, amount)` signed by `keys` under `rule_ids`.
    pub fn try_pay(
        &self,
        keys: &[&SigningKey],
        rule_ids: &[u32],
        to: &Address,
        amount: i128,
    ) -> bool {
        let entry = self.signed_entry(keys, rule_ids, self.transfer_invocation(to, amount));
        self.e.set_auths(&[entry]);
        let ok = self
            .token_client()
            .try_transfer(&self.account, to, &amount)
            .is_ok();
        self.e.set_auths(&[]);
        ok
    }

    /// Agent pays under its `:pay` rule.
    pub fn agent_pay(&self, to: &Address, amount: i128) -> bool {
        self.try_pay(&[&self.agent], &[self.pay_rule], to, amount)
    }

    /// Runs `f` with an owner-signed entry for `invocation` (rule 0 for each context).
    pub fn as_owner<T>(
        &self,
        invocation: SorobanAuthorizedInvocation,
        contexts: usize,
        f: impl FnOnce() -> T,
    ) -> T {
        let ids = std::vec![OWNER_RULE; contexts];
        let entry = self.signed_entry(&[&self.owner], &ids, invocation);
        self.e.set_auths(&[entry]);
        let out = f();
        self.e.set_auths(&[]);
        out
    }
}

pub fn to_scval<T: IntoVal<Env, Val>>(e: &Env, value: T) -> ScVal {
    ScVal::try_from_val(e, &value.into_val(e)).unwrap()
}

pub fn contract_fn(
    contract: &Address,
    name: &str,
    args: std::vec::Vec<ScVal>,
) -> SorobanAuthorizedInvocation {
    SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
            contract_address: contract.into(),
            function_name: name.try_into().unwrap(),
            args: args.try_into().unwrap(),
        }),
        sub_invocations: VecM::default(),
    }
}

pub fn signature_payload(
    e: &Env,
    nonce: i64,
    expiration: u32,
    invocation: &SorobanAuthorizedInvocation,
) -> BytesN<32> {
    let preimage = HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
        network_id: XdrHash(e.ledger().get().network_id),
        nonce,
        signature_expiration_ledger: expiration,
        invocation: invocation.clone(),
    });
    let xdr = preimage.to_xdr(Limits::none()).unwrap();
    e.crypto().sha256(&Bytes::from_slice(e, &xdr)).to_bytes()
}

#[test]
fn harness_deploys_wallet_with_agent_rule() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    assert_eq!(w.balance(&w.account), 1_000 * UNIT);
    let rule = w.account_client().get_context_rule(&w.pay_rule);
    assert_eq!(rule.policies.len(), 1);
    assert_eq!(rule.signers.len(), 1);
    let m = w.policy_client().get_mandate(&w.account, &w.pay_rule);
    assert_eq!(m.token, w.token);
    assert_eq!(m.budget, 5 * UNIT);
}

#[test]
fn harness_signs_a_real_agent_payment() {
    let w = Wallet::new(5 * UNIT, 1_000, 2 * UNIT);
    let to = Address::generate(&w.e);
    assert!(w.agent_pay(&to, UNIT));
    assert_eq!(w.balance(&to), UNIT);
}
