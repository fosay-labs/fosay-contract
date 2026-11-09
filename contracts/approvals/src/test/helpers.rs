//! Harness: canonical OZ smart account (wasm fixture) + Ed25519 verifier + SAC +
//! `fosay-mandate-policy` on the agent `:pay` rule + this queue on the agent `:ask`
//! rule, with real signed auth entries (incl. nested sub-invocations for `execute`).
#![allow(dead_code)]

extern crate std;

use core::cell::Cell;

use ed25519_dalek::{Signer as _, SigningKey};
use fosay_mandate_policy::{MandateParams, MandatePolicy, MandatePolicyClient};
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

use crate::contract::{Approvals, ApprovalsClient};

pub mod oz_account {
    soroban_sdk::contractimport!(file = "../../tests/fixtures/oz_smart_account_v0_8_0.wasm");
}
pub mod oz_ed25519 {
    soroban_sdk::contractimport!(file = "../../tests/fixtures/oz_ed25519_verifier.wasm");
}

pub const UNIT: i128 = 10_000_000;
pub const START_LEDGER: u32 = 1_000;
pub const OWNER_RULE: u32 = 0;
pub const MAX_PENDING: u32 = 3;
pub const MAX_TTL: u32 = 1_000;

pub struct Fosay {
    pub e: Env,
    pub account: Address,
    pub verifier: Address,
    pub policy: Address,
    pub approvals: Address,
    pub token: Address,
    pub owner: SigningKey,
    pub agent: SigningKey,
    pub stranger: SigningKey,
    pub pay_rule: u32,
    pub ask_rule: u32,
    nonce: Cell<i64>,
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn signer(e: &Env, verifier: &Address, k: &SigningKey) -> Signer {
    Signer::External(
        verifier.clone(),
        Bytes::from_array(e, k.verifying_key().as_bytes()),
    )
}

fn oz_signer(e: &Env, verifier: &Address, k: &SigningKey) -> oz_account::Signer {
    oz_account::Signer::External(
        verifier.clone(),
        Bytes::from_array(e, k.verifying_key().as_bytes()),
    )
}

pub fn zero_hash(e: &Env) -> BytesN<32> {
    BytesN::from_array(e, &[0; 32])
}

impl Fosay {
    /// Wallet with 1,000 tokens; agent `:pay` rule (mandate: budget/period/max_per_tx)
    /// and agent `:ask` rule on the approvals queue.
    pub fn new(budget: i128, period: u32, max_per_tx: i128) -> Fosay {
        let e = Env::default();
        e.cost_estimate().budget().reset_unlimited();
        e.ledger().set_sequence_number(START_LEDGER);
        let verifier = e.register(oz_ed25519::WASM, ());
        let policy = e.register(MandatePolicy, ());
        let approvals = e.register(Approvals, (MAX_PENDING, MAX_TTL));
        let (owner, agent, stranger) = (key(11), key(22), key(33));
        let account = e.register(
            oz_account::WASM,
            (
                vec![&e, oz_signer(&e, &verifier, &owner)],
                Map::<Address, Val>::new(&e),
            ),
        );
        let token = e
            .register_stellar_asset_contract_v2(Address::generate(&e))
            .address();

        e.mock_all_auths();
        token::StellarAssetClient::new(&e, &token).mint(&account, &(1_000 * UNIT));
        let acct = oz_account::Client::new(&e, &account);
        let agent_signer = vec![&e, oz_signer(&e, &verifier, &agent)];
        let mut pay_policies: Map<Address, Val> = Map::new(&e);
        let p = MandateParams {
            budget,
            period_ledgers: period,
            max_per_tx,
            recipients: Vec::new(&e),
            mandate_hash: BytesN::from_array(&e, &[7; 32]),
        };
        pay_policies.set(policy.clone(), p.into_val(&e));
        let pay_rule = acct
            .add_context_rule(
                &oz_account::ContextRuleType::CallContract(token.clone()),
                &String::from_str(&e, "agt1:pay"),
                &None,
                &agent_signer,
                &pay_policies,
            )
            .id;
        let ask_rule = acct
            .add_context_rule(
                &oz_account::ContextRuleType::CallContract(approvals.clone()),
                &String::from_str(&e, "agt1:ask"),
                &None,
                &agent_signer,
                &Map::new(&e),
            )
            .id;
        e.set_auths(&[]);
        Fosay {
            e,
            account,
            verifier,
            policy,
            approvals,
            token,
            owner,
            agent,
            stranger,
            pay_rule,
            ask_rule,
            nonce: Cell::new(1),
        }
    }

    pub fn q(&self) -> ApprovalsClient<'_> {
        ApprovalsClient::new(&self.e, &self.approvals)
    }

    pub fn policy_client(&self) -> MandatePolicyClient<'_> {
        MandatePolicyClient::new(&self.e, &self.policy)
    }

    pub fn account_client(&self) -> oz_account::Client<'_> {
        oz_account::Client::new(&self.e, &self.account)
    }

    pub fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.e, &self.token).balance(who)
    }

    pub fn advance(&self, ledgers: u32) {
        let s = self.e.ledger().sequence();
        self.e.ledger().set_sequence_number(s + ledgers);
    }

    pub fn signed_entry(
        &self,
        keys: &[&SigningKey],
        rule_ids: &[u32],
        invocation: SorobanAuthorizedInvocation,
    ) -> SorobanAuthorizationEntry {
        let e = &self.e;
        let nonce = self.nonce.get();
        self.nonce.set(nonce + 1);
        let expiration = e.ledger().sequence() + 100;
        let preimage = HashIdPreimage::SorobanAuthorization(HashIdPreimageSorobanAuthorization {
            network_id: XdrHash(e.ledger().get().network_id),
            nonce,
            signature_expiration_ledger: expiration,
            invocation: invocation.clone(),
        });
        let xdr = preimage.to_xdr(Limits::none()).unwrap();
        let payload = e.crypto().sha256(&Bytes::from_slice(e, &xdr)).to_bytes();
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
        SorobanAuthorizationEntry {
            credentials: SorobanCredentials::Address(SorobanAddressCredentials {
                address: (&self.account).into(),
                nonce,
                signature_expiration_ledger: expiration,
                signature: to_scval(
                    e,
                    AuthPayload {
                        signers,
                        context_rule_ids: ids,
                    },
                ),
            }),
            root_invocation: invocation,
        }
    }

    pub fn transfer_inv(&self, to: &Address, amount: i128) -> SorobanAuthorizedInvocation {
        contract_fn(
            &self.token,
            "transfer",
            std::vec![
                to_scval(&self.e, self.account.clone()),
                to_scval(&self.e, to.clone()),
                to_scval(&self.e, amount),
            ],
            std::vec![],
        )
    }

    pub fn propose_inv(
        &self,
        to: &Address,
        amount: i128,
        memo: &String,
        ttl: u32,
    ) -> SorobanAuthorizedInvocation {
        contract_fn(
            &self.approvals,
            "propose",
            std::vec![
                to_scval(&self.e, self.account.clone()),
                to_scval(&self.e, self.token.clone()),
                to_scval(&self.e, to.clone()),
                to_scval(&self.e, amount),
                to_scval(&self.e, zero_hash(&self.e)),
                to_scval(&self.e, memo.clone()),
                to_scval(&self.e, ttl),
            ],
            std::vec![],
        )
    }

    /// `execute(id)` with the nested `transfer` the owner is approving.
    pub fn execute_inv(&self, id: u64) -> SorobanAuthorizedInvocation {
        let intent = self.q().get_intent(&id);
        contract_fn(
            &self.approvals,
            "execute",
            std::vec![to_scval(&self.e, id)],
            std::vec![self.transfer_inv(&intent.to, intent.amount)],
        )
    }

    /// Agent proposes under its `:ask` rule; returns the result of `try_propose`.
    pub fn try_agent_propose(&self, to: &Address, amount: i128, ttl: u32) -> Result<u64, ()> {
        let memo = String::from_str(&self.e, "need approval");
        let entry = self.signed_entry(
            &[&self.agent],
            &[self.ask_rule],
            self.propose_inv(to, amount, &memo, ttl),
        );
        self.e.set_auths(&[entry]);
        let r = self.q().try_propose(
            &self.account,
            &self.token,
            to,
            &amount,
            &zero_hash(&self.e),
            &memo,
            &ttl,
        );
        self.e.set_auths(&[]);
        match r {
            Ok(Ok(id)) => Ok(id),
            _ => Err(()),
        }
    }

    pub fn agent_propose(&self, to: &Address, amount: i128) -> u64 {
        self.try_agent_propose(to, amount, 500).unwrap()
    }

    /// Runs `execute(id)` with an entry signed by `keys` under `rule_ids`.
    pub fn try_execute(&self, keys: &[&SigningKey], rule_ids: &[u32], id: u64) -> bool {
        let entry = self.signed_entry(keys, rule_ids, self.execute_inv(id));
        self.e.set_auths(&[entry]);
        let ok = self.q().try_execute(&id).is_ok();
        self.e.set_auths(&[]);
        ok
    }

    /// Owner approves on their phone: one entry, rules `[owner, owner]`.
    pub fn owner_execute(&self, id: u64) -> bool {
        self.try_execute(&[&self.owner], &[OWNER_RULE, OWNER_RULE], id)
    }

    /// Agent tries to execute: `[:ask, :pay]`.
    pub fn agent_execute(&self, id: u64) -> bool {
        self.try_execute(&[&self.agent], &[self.ask_rule, self.pay_rule], id)
    }

    /// `cancel(id)` signed by `keys` under `rule`.
    pub fn try_cancel(&self, keys: &[&SigningKey], rule: u32, id: u64) -> bool {
        let inv = contract_fn(
            &self.approvals,
            "cancel",
            std::vec![to_scval(&self.e, id)],
            std::vec![],
        );
        let entry = self.signed_entry(keys, &[rule], inv);
        self.e.set_auths(&[entry]);
        let ok = self.q().try_cancel(&id).is_ok();
        self.e.set_auths(&[]);
        ok
    }
}

pub fn to_scval<T: IntoVal<Env, Val>>(e: &Env, value: T) -> ScVal {
    ScVal::try_from_val(e, &value.into_val(e)).unwrap()
}

pub fn contract_fn(
    contract: &Address,
    name: &str,
    args: std::vec::Vec<ScVal>,
    subs: std::vec::Vec<SorobanAuthorizedInvocation>,
) -> SorobanAuthorizedInvocation {
    SorobanAuthorizedInvocation {
        function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
            contract_address: contract.into(),
            function_name: name.try_into().unwrap(),
            args: args.try_into().unwrap(),
        }),
        sub_invocations: subs.try_into().unwrap_or(VecM::default()),
    }
}

pub fn err(
    code: crate::errors::ApprovalsError,
) -> Result<soroban_sdk::Error, soroban_sdk::InvokeError> {
    Ok(soroban_sdk::Error::from(code))
}

#[test]
fn harness_builds_wallet_with_pay_and_ask_rules() {
    let f = Fosay::new(5 * UNIT, 1_000, 2 * UNIT);
    assert_eq!(f.balance(&f.account), 1_000 * UNIT);
    assert_eq!(
        f.account_client()
            .get_context_rule(&f.ask_rule)
            .policies
            .len(),
        0
    );
    assert_eq!(
        f.account_client()
            .get_context_rule(&f.pay_rule)
            .policies
            .len(),
        1
    );
    let to = Address::generate(&f.e);
    let id = f.agent_propose(&to, 10 * UNIT);
    assert_eq!(id, 1);
    assert!(f.owner_execute(id));
    assert_eq!(f.balance(&to), 10 * UNIT);
}
