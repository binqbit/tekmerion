use anchor_lang::prelude::Pubkey;
use solana_instruction::Instruction;

use super::{CaseAddress, instruction};

/// The beneficiary signs; the program checks ownership of the destination.
pub struct ClaimDestination {
    pub owner: Pubkey,
    pub destination: Pubkey,
}

/// Supply the Case's authenticated author to derive its payment marker for the
/// vault-backing check. The beneficiary's receipt determines the payout role.
/// An author claim supplies the same receipt in both receipt account positions.
pub fn claim(case: CaseAddress, author: Pubkey, claim: ClaimDestination) -> Instruction {
    instruction(
        tekmerion::accounts::Claim {
            owner: claim.owner,

            case: case.address(),
            author_receipt: case.receipt(&author),
            receipt: case.receipt(&claim.owner),

            mint: tekmerion::USDC_MINT,
            vault: case.vault(),
            destination: claim.destination,

            token_program: anchor_spl::token::ID,
        },
        tekmerion::instruction::Claim {},
    )
}
