use std::collections::BTreeSet;

use solana_account::{Account, AccountSharedData};
use solana_address::Address;
use solana_clock::Clock;
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_program_test::ProgramTestContext;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction_error::TransactionError;
use tokio::runtime::Runtime;

use super::{Actor, BALANCE, MAIN, pda, rules, setup};

/// Each scenario owns genuine program execution, actor keys, custody and clock.
pub struct TestContext {
    pub context: ProgramTestContext,
    pub keys: Vec<Keypair>,
    pub protected: BTreeSet<Address>,
    pub policy: Address,
    pub subject: Address,
    pub case: Address,
    pub vault: Address,
    pub authority: Address,
    pub authority_actor: Actor,
    pub token_accounts: Vec<Address>,
    pub(super) runtime: Runtime,
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TestContext {
    pub fn new() -> Self {
        Self::with_program_test(
            setup::programs::program_test(),
            Actor::ClientAuthority,
            rules(),
            BALANCE,
        )
    }

    pub fn with_wallet_authority(authority: Actor) -> Self {
        Self::with_program_test(setup::programs::program_test(), authority, rules(), BALANCE)
    }

    /// Explicit Policy and token budget for journeys at ordinary USDC scale.
    pub fn with_rules(rules: tekmerion::PolicyRules, token_balance: u64) -> Self {
        Self::with_program_test(
            setup::programs::program_test(),
            Actor::ClientAuthority,
            rules,
            token_balance,
        )
    }

    pub fn get_account(&self, address: &Address) -> Option<Account> {
        self.runtime
            .block_on(self.context.banks_client.get_account(*address))
            .expect("read bank account")
    }

    pub fn set_account(&mut self, address: Address, account: Account) {
        self.context
            .set_account(&address, &AccountSharedData::from(account));
    }

    pub fn clock(&self) -> Clock {
        self.runtime
            .block_on(self.context.banks_client.get_sysvar())
            .expect("read bank Clock")
    }

    pub fn set_clock(&mut self, clock: &Clock) {
        self.context.set_sysvar(clock);
    }

    pub fn fund(&mut self, address: Address, lamports: u64) {
        self.runtime
            .block_on(setup::context::fund(&mut self.context, address, lamports))
            .expect("fund account through System program");
    }

    pub fn latest_blockhash(&self) -> Hash {
        self.context.last_blockhash
    }

    /// Wait for the bank's next recent blockhash without freezing or replacing
    /// the bank. Its ticker leaves the explicitly injected Clock unchanged.
    pub fn new_blockhash(&mut self) -> Hash {
        self.runtime
            .block_on(self.context.get_new_latest_blockhash())
            .expect("next bank blockhash")
    }

    pub fn transaction_status(
        &self,
        signature: &Signature,
    ) -> Option<Result<(), TransactionError>> {
        self.runtime
            .block_on(self.context.banks_client.get_transaction_status(*signature))
            .expect("read bank signature status")
            .map(|status| status.err.map_or(Ok(()), Err))
    }

    pub fn creation_signers(&self) -> Vec<Actor> {
        vec![Actor::Author, self.authority_actor]
    }

    pub fn actor(&self, actor: Actor) -> Address {
        self.keys[actor as usize].pubkey()
    }

    pub fn token(&self, actor: Actor) -> Address {
        self.token_accounts[actor as usize]
    }

    pub fn author_receipt(&self) -> Address {
        self.receipt(Actor::Author)
    }

    pub fn receipt(&self, actor: Actor) -> Address {
        pda(
            &[b"receipt", self.case.as_ref(), self.actor(actor).as_ref()],
            MAIN,
        )
    }
}
