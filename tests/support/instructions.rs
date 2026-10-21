use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use tekmerion_sdk::instructions::{self as sdk, CaseAddress, ClaimDestination, Contribution};

use super::{Actor, TestContext, pk};

/// Locate the intended attack target without depending on the public account order.
/// Ambiguous or absent identities fail before a mutated transaction is constructed.
pub fn account_index(instruction: &Instruction, address: Address) -> usize {
    let mut matches = instruction
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| account.pubkey == address);
    let index = matches
        .next()
        .expect("instruction includes the target account")
        .0;
    assert!(
        matches.next().is_none(),
        "account identity must select exactly one role"
    );
    index
}

pub fn account_meta_mut(instruction: &mut Instruction, address: Address) -> &mut AccountMeta {
    let index = account_index(instruction, address);
    &mut instruction.accounts[index]
}

/// Select the writable role when a shared claim repeats the author receipt.
pub fn writable_account_meta_mut(
    instruction: &mut Instruction,
    address: Address,
) -> &mut AccountMeta {
    let mut matches = instruction
        .accounts
        .iter()
        .enumerate()
        .filter(|(_, account)| account.pubkey == address && account.is_writable);
    let index = matches
        .next()
        .expect("instruction includes the writable role")
        .0;
    assert!(
        matches.next().is_none(),
        "writable identity must select exactly one role"
    );
    &mut instruction.accounts[index]
}

impl TestContext {
    pub fn case_address(&self) -> CaseAddress {
        CaseAddress {
            policy: pk(self.policy),
            subject: pk(self.subject),
            author: pk(self.actor(Actor::Author)),
            nonce: 0,
        }
    }

    fn claim_destination(&self, owner: Actor) -> ClaimDestination {
        ClaimDestination {
            owner: pk(self.actor(owner)),
            destination: pk(self.token(owner)),
        }
    }

    pub fn create_policy_ix(&self, rules: tekmerion::PolicyRules) -> Instruction {
        sdk::create_policy(sdk::CreatePolicy {
            payer: pk(self.actor(Actor::Payer)),
            admin: pk(self.actor(Actor::Admin)),
            client_authority: pk(self.authority),
            identifier: 0,
            rules,
        })
    }
    pub fn update_policy_ix(&self, rules: tekmerion::PolicyRules) -> Instruction {
        sdk::update_policy(pk(self.actor(Actor::Admin)), 0, rules)
    }
    pub fn submit_case_ix(&self, author_principal: u64) -> Instruction {
        sdk::create_case(
            self.case_address(),
            sdk::CreateCase {
                payer: pk(self.actor(Actor::Payer)),
                client_authority: pk(self.authority),
                author: pk(self.actor(Actor::Author)),
                source: pk(self.token(Actor::Author)),
                funding: tekmerion::CaseFunding::Bonded {
                    principal: author_principal,
                },
                agreement_hash: [0xa1; 32],
                result_hash: [0x91; 32],
            },
        )
    }
    pub fn direct_case_ix(&self, expected_fee: u64) -> Instruction {
        sdk::create_case(
            self.case_address(),
            sdk::CreateCase {
                payer: pk(self.actor(Actor::Payer)),
                client_authority: pk(self.authority),
                author: pk(self.actor(Actor::Author)),
                source: pk(self.token(Actor::Author)),
                funding: tekmerion::CaseFunding::DirectAudit { expected_fee },
                agreement_hash: [0xa1; 32],
                result_hash: [0x91; 32],
            },
        )
    }
    pub fn contribute_ix(&self, owner: Actor, requested: u64) -> Instruction {
        sdk::contribute(
            self.case_address(),
            Contribution {
                payer: pk(self.actor(Actor::Payer)),
                owner: pk(self.actor(owner)),
                source: pk(self.token(owner)),
            },
            requested,
        )
    }
    pub fn finalize_ix(&self) -> Instruction {
        sdk::finalize_unchallenged(self.case_address())
    }
    pub fn author_claim_ix(&self) -> Instruction {
        sdk::claim(
            self.case_address(),
            self.case_state().author,
            self.claim_destination(Actor::Author),
        )
    }
    pub fn contributor_claim_ix(&self, owner: Actor) -> Instruction {
        sdk::claim(
            self.case_address(),
            self.case_state().author,
            self.claim_destination(owner),
        )
    }

    pub fn finalize_timeout_ix(&self) -> Instruction {
        sdk::finalize_timeout(self.case_address())
    }
}
