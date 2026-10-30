//! Synthetic oracle keys with the unmodified, pinned Keystone Forwarder SBF.
//! This is real local signature verification, not a live DON execution.
use super::{Actor, BALANCE, SYSTEM, TestContext, pk, rules, setup};
use anchor_lang::prelude::Pubkey;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use tekmerion_chainlink_sdk::{
    confidential::{ConfidentialProfile, ConfidentialRequest},
    program as adapter,
};

#[path = "forwarder.rs"]
pub mod forwarder;
pub use forwarder::{
    CONFIG_VERSION, DON, FORWARDER, discriminator, oracle_address, oracle_keys, serialize, sha,
};

pub struct ForwarderContext {
    pub bank: TestContext,
    pub state: Pubkey,
    pub profile: Pubkey,
    pub request: Pubkey,
    pub oracles: Pubkey,
}
impl ForwarderContext {
    pub fn configured() -> Self {
        let mut program = setup::programs::program_test();
        program.add_program("chainlink_auditor", adapter::ID, None);
        program.add_program("keystone_forwarder", FORWARDER, None);
        let mut bank =
            TestContext::with_program_test(program, Actor::ClientAuthority, rules(), BALANCE);
        let profile = ConfidentialProfile::address(&pk(bank.policy)).0;
        let mut rules = rules();
        rules.auditors[0].authority = profile;
        bank.ok(&[bank.update_policy_ix(rules)], &[Actor::Admin]);
        let state = Keypair::new_from_array([0xf1; 32]);
        let ix = Instruction {
            program_id: FORWARDER,
            accounts: vec![
                AccountMeta::new(state.pubkey(), true),
                AccountMeta::new(pk(bank.actor(Actor::Admin)), true),
                AccountMeta::new_readonly(pk(SYSTEM), false),
            ],
            data: discriminator("initialize"),
        };
        bank.send(&[ix], &[Actor::Admin], &[&state]).unwrap();
        let config_id = ((u64::from(DON) << 32) | u64::from(CONFIG_VERSION)).to_be_bytes();
        let oracles = Pubkey::find_program_address(
            &[b"config", state.pubkey().as_ref(), &config_id],
            &FORWARDER,
        )
        .0;
        let mut data = discriminator("init_oracles_config");
        data.extend(DON.to_le_bytes());
        data.extend(CONFIG_VERSION.to_le_bytes());
        data.push(1);
        let mut addresses: Vec<_> = oracle_keys().iter().map(oracle_address).collect();
        addresses.sort();
        data.extend(serialize(&addresses));
        bank.ok(
            &[Instruction {
                program_id: FORWARDER,
                accounts: vec![
                    AccountMeta::new_readonly(state.pubkey(), false),
                    AccountMeta::new(oracles, false),
                    AccountMeta::new(pk(bank.actor(Actor::Admin)), true),
                    AccountMeta::new_readonly(pk(SYSTEM), false),
                ],
                data,
            }],
            &[Actor::Admin],
        );
        let request = ConfidentialRequest::address(&profile, &pk(bank.case)).0;
        Self {
            bank,
            state: state.pubkey(),
            profile,
            request,
            oracles,
        }
    }
}
