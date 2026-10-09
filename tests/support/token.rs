use solana_address::Address;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::instruction;

use super::{Actor, MINT, TOKEN, TestContext, pk, setup::token};

impl TestContext {
    pub fn create_mint(&mut self, mint: &Keypair) {
        let authority = self.actor(Actor::Admin);
        self.runtime
            .block_on(token::create_mint(
                &mut self.context,
                mint,
                authority,
                Some(authority),
                6,
            ))
            .expect("genuine Token mint initialization");
        self.protected.insert(mint.pubkey());
    }

    pub fn create_token_account(&mut self, seed: u8, owner: Address, mint: Address) -> Address {
        let key = Keypair::new_from_array([seed; 32]);
        self.runtime
            .block_on(token::create_account(&mut self.context, &key, owner, mint))
            .expect("genuine Token account initialization");
        self.protected.insert(key.pubkey());
        key.pubkey()
    }

    pub fn transfer_ix(
        &self,
        source: Address,
        destination: Address,
        owner: Actor,
        amount: u64,
    ) -> Instruction {
        instruction::transfer_checked(
            &pk(TOKEN),
            &pk(source),
            &pk(MINT),
            &pk(destination),
            &pk(self.actor(owner)),
            &[],
            amount,
            6,
        )
        .unwrap()
    }

    pub fn freeze(&mut self, account: Address, frozen: bool) {
        let ix = if frozen {
            instruction::freeze_account(
                &pk(TOKEN),
                &pk(account),
                &pk(MINT),
                &pk(self.actor(Actor::Admin)),
                &[],
            )
        } else {
            instruction::thaw_account(
                &pk(TOKEN),
                &pk(account),
                &pk(MINT),
                &pk(self.actor(Actor::Admin)),
                &[],
            )
        }
        .unwrap();
        self.ok(&[ix], &[Actor::Admin]);
    }
}
