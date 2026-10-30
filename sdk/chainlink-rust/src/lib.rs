//! Unsigned confidential auditor builders and strict supplied-account validation.
//! No RPC, wallet, credentials or launch transport is bundled here.
use anchor_lang::{InstructionData, ToAccountMetas};
pub use chainlink_auditor::{self as program, ID};
use solana_instruction::Instruction;
pub use tekmerion_sdk::instructions::CaseAddress;
pub mod confidential;
fn instruction(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}
