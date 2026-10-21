use anchor_lang::{AnchorDeserialize, AnchorSerialize, prelude::Pubkey, system_program};
use solana_instruction::{AccountMeta, Instruction};
use tekmerion::{AuditAnswer, AuditReason, Case, StakeReceipt};

use super::instruction;

/// Immutable Case identity. Derived addresses use the program's own PDA helpers.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaseAddress {
    pub policy: Pubkey,
    pub subject: Pubkey,
    pub author: Pubkey,
    pub nonce: u64,
}

impl CaseAddress {
    pub fn address(self) -> Pubkey {
        Case::address(&self.policy, &self.subject, &self.author, self.nonce).0
    }

    pub fn vault(self) -> Pubkey {
        Case::vault_address(&self.address()).0
    }

    pub fn receipt(self, owner: &Pubkey) -> Pubkey {
        StakeReceipt::address(&self.address(), owner).0
    }
}

/// Bonded submission authorized by the configured client and the token owner.
/// The caller owns client-specific subject and current-Case validation.
pub struct CreateCase {
    // Signers.
    pub payer: Pubkey,
    pub client_authority: Pubkey,
    pub author: Pubkey,

    // Token custody.
    pub source: Pubkey,

    // Assertion inputs.
    pub author_principal: u64,
    pub agreement_hash: [u8; 32],
    pub result_hash: [u8; 32],
}

/// Later attempts include the canonical predecessor as a readonly account.
/// The program verifies its final result and relationship at execution.
pub fn create_case(case: CaseAddress, input: CreateCase) -> Instruction {
    let mut ix = instruction(
        tekmerion::accounts::CreateCase {
            payer: input.payer,
            client_authority: input.client_authority,
            author: input.author,

            policy: case.policy,
            subject: case.subject,
            case: case.address(),
            author_receipt: case.receipt(&input.author),

            mint: tekmerion::USDC_MINT,
            source: input.source,
            vault: case.vault(),

            token_program: anchor_spl::token::ID,
            system_program: system_program::ID,
        },
        tekmerion::instruction::CreateCase {
            nonce: case.nonce,
            author_principal: input.author_principal,
            agreement_hash: input.agreement_hash,
            result_hash: input.result_hash,
        },
    );
    if let Some(nonce) = case.nonce.checked_sub(1) {
        ix.accounts.push(AccountMeta::new_readonly(
            Case::address(&case.policy, &case.subject, &case.author, nonce).0,
            false,
        ));
    }
    ix
}

/// Funding rent and token ownership are separate signer responsibilities.
pub struct Contribution {
    pub payer: Pubkey,
    pub owner: Pubkey,

    pub source: Pubkey,
}

/// The program caps actual credit at remaining capacity at execution time.
pub fn contribute(case: CaseAddress, input: Contribution, requested: u64) -> Instruction {
    instruction(
        tekmerion::accounts::Contribute {
            payer: input.payer,
            owner: input.owner,

            case: case.address(),
            policy: case.policy,
            receipt: case.receipt(&input.owner),

            mint: tekmerion::USDC_MINT,
            source: input.source,
            vault: case.vault(),

            token_program: anchor_spl::token::ID,
            system_program: system_program::ID,
        },
        tekmerion::instruction::Contribute { requested },
    )
}

pub fn finalize_unchallenged(case: CaseAddress) -> Instruction {
    instruction(
        tekmerion::accounts::FinalizeUnchallenged {
            case: case.address(),
            vault: case.vault(),
        },
        tekmerion::instruction::FinalizeUnchallenged {},
    )
}

/// Record the signer's immutable audit vote. The vote that reaches quorum also
/// settles and pays the winning auditors atomically. Supply their USDC accounts
/// in configured auditor-index order only for that vote; otherwise pass `&[]`.
pub fn submit_audit(
    case: CaseAddress,
    auditor: Pubkey,
    answer: AuditAnswer,
    reason: AuditReason,
    fee_destinations: &[Pubkey],
) -> Instruction {
    let mut ix = instruction(
        tekmerion::accounts::SubmitAudit {
            auditor,

            case: case.address(),
            policy: case.policy,

            mint: tekmerion::USDC_MINT,
            vault: case.vault(),

            token_program: anchor_spl::token::ID,
        },
        tekmerion::instruction::SubmitAudit { answer, reason },
    );
    ix.accounts.extend(
        fee_destinations
            .iter()
            .copied()
            .map(|destination| AccountMeta::new(destination, false)),
    );
    ix
}

pub fn finalize_timeout(case: CaseAddress) -> Instruction {
    instruction(
        tekmerion::accounts::FinalizeTimeout {
            case: case.address(),
            vault: case.vault(),
        },
        tekmerion::instruction::FinalizeTimeout {},
    )
}
