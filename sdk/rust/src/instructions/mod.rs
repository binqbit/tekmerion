mod case;
mod claims;
mod policy;

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_instruction::Instruction;

pub use case::{
    CaseAddress, Contribution, CreateCase, contribute, create_case, finalize_timeout,
    finalize_unchallenged, submit_audit,
};
pub use claims::{ClaimDestination, claim};
pub use policy::{CreatePolicy, acquire_policy, create_policy, update_policy};

fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: tekmerion::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}
