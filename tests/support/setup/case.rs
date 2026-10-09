//! Deterministic local identities and default Policy inputs.
use std::collections::BTreeSet;

use anchor_lang::prelude::Pubkey;
use solana_clock::Clock;
use tokio::runtime::Builder;

use super::super::{TestContext, setup};
use super::token;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_program_test::ProgramTest;
use solana_signer::Signer;

pub const START: i64 = 1_000;
pub const BALANCE: u64 = 10_000;
pub const BOND: u64 = 125;
pub const TARGET: u64 = 100;
pub const DURATION: u64 = 60;
pub const MAIN: Address = Address::new_from_array(tekmerion::ID.to_bytes());
pub const TOKEN: Address = Address::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const SYSTEM: Address = Address::new_from_array([0; 32]);
pub const MINT: Address = Address::new_from_array(tekmerion::USDC_MINT.to_bytes());

#[derive(Clone, Copy, Debug)]
pub enum Actor {
    Payer = 0,
    Admin = 1,
    Author = 2,
    Alice = 3,
    Bob = 4,
    Outsider = 5,
    FeeRecipient = 6,
    Auditor1 = 7,
    Auditor2 = 8,
    Auditor3 = 9,
    Auditor4 = 10,
    Auditor5 = 11,
    FeeRecipient2 = 12,
    FeeRecipient3 = 13,
    FeeRecipient4 = 14,
    FeeRecipient5 = 15,
    ClientAuthority = 16,
}

pub fn pk(address: Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

pub fn pda(seeds: &[&[u8]], program: Address) -> Address {
    Address::find_program_address(seeds, &program).0
}

pub const AUDITORS: [Actor; 5] = [
    Actor::Auditor1,
    Actor::Auditor2,
    Actor::Auditor3,
    Actor::Auditor4,
    Actor::Auditor5,
];
pub const FEE_RECIPIENTS: [Actor; 5] = [
    Actor::FeeRecipient,
    Actor::FeeRecipient2,
    Actor::FeeRecipient3,
    Actor::FeeRecipient4,
    Actor::FeeRecipient5,
];

pub fn rules() -> tekmerion::PolicyRules {
    committee_rules(1, 1)
}

pub fn committee_rules(threshold: u8, count: usize) -> tekmerion::PolicyRules {
    tekmerion::PolicyRules {
        auditors: (0..count)
            .map(|index| tekmerion::AuditorConfig {
                authority: pk(Keypair::new_from_array([AUDITORS[index] as u8 + 1; 32]).pubkey()),
                fee_recipient: pk(
                    Keypair::new_from_array([FEE_RECIPIENTS[index] as u8 + 1; 32]).pubkey(),
                ),
            })
            .collect(),
        threshold,
        author_bond: 100,
        challenge_target: TARGET,
        audit_fee: 7,
        funding_duration: DURATION,
        audit_timeout: 300,
    }
}

/// An ordinary wallet address is sufficient to identify an opaque audit subject.
pub fn subject_address(identifier: u64) -> Address {
    let mut seed = [0x56; 32];
    seed[..8].copy_from_slice(&identifier.to_le_bytes());
    Keypair::new_from_array(seed).pubkey()
}

impl TestContext {
    /// Add custom programs or state to the ordinary ProgramTest builder before
    /// starting a scenario with the standard wallets and token balances.
    pub fn with_program_test(
        mut program: ProgramTest,
        authority_actor: Actor,
        rules: tekmerion::PolicyRules,
        token_balance: u64,
    ) -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let keys: Vec<_> = (1..=17)
            .map(|seed| Keypair::new_from_array([seed; 32]))
            .collect();
        for key in &keys {
            setup::context::add_system_account(&mut program, key.pubkey(), 2_000_000_000);
        }
        let policy = pda(
            &[
                b"policy",
                keys[Actor::Admin as usize].pubkey().as_ref(),
                &0_u64.to_le_bytes(),
            ],
            MAIN,
        );
        let subject = subject_address(0);
        setup::context::add_system_account(&mut program, subject, 1_000_000);
        let case = pda(
            &[
                b"case",
                policy.as_ref(),
                subject.as_ref(),
                &0_u64.to_le_bytes(),
            ],
            MAIN,
        );
        let vault = pda(&[b"vault", case.as_ref()], MAIN);
        let authority = keys[authority_actor as usize].pubkey();
        let mut protected: BTreeSet<_> = keys.iter().map(Signer::pubkey).collect();
        protected.extend([policy, subject, case, vault, authority, MINT]);
        let clock = Clock {
            slot: 100,
            unix_timestamp: START,
            ..Clock::default()
        };
        let context = runtime.block_on(setup::context::start(program, clock));
        let mut ctx = Self {
            context,
            keys,
            protected,
            policy,
            subject,
            case,
            vault,
            authority,
            authority_actor,
            token_accounts: Vec::new(),
            runtime,
        };
        ctx.protected.insert(ctx.author_receipt());
        ctx.initialize_tokens(token_balance);
        ctx.ok(&[ctx.create_policy_ix(rules)], &[Actor::Admin]);
        assert_eq!(ctx.policy_state().active_uses, 0);
        ctx
    }
}

impl TestContext {
    pub fn initialize_tokens(&mut self, balance: u64) {
        let mint = Keypair::new_from_array([0x57; 32]);
        self.create_mint(&mint);
        // Install genuinely initialized Token state at the selected USDC address.
        // Authorities and balances are synthetic fixtures; no Circle key is used.
        self.runtime
            .block_on(token::copy_mint(&mut self.context, mint.pubkey(), MINT))
            .expect("install synthetic USDC mint");
        self.protected.insert(MINT);
        for actor in [
            Actor::Payer,
            Actor::Admin,
            Actor::Author,
            Actor::Alice,
            Actor::Bob,
            Actor::Outsider,
            Actor::FeeRecipient,
            Actor::Auditor1,
            Actor::Auditor2,
            Actor::Auditor3,
            Actor::Auditor4,
            Actor::Auditor5,
            Actor::FeeRecipient2,
            Actor::FeeRecipient3,
            Actor::FeeRecipient4,
            Actor::FeeRecipient5,
            Actor::ClientAuthority,
        ] {
            let token = self.create_token_account(20 + actor as u8, self.actor(actor), MINT);
            self.token_accounts.push(token);
            self.runtime
                .block_on(token::mint_to(
                    &mut self.context,
                    MINT,
                    token,
                    &self.keys[Actor::Admin as usize],
                    balance,
                    6,
                ))
                .expect("seed token balance through SPL Token");
        }
    }
}
