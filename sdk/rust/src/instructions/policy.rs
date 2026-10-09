use anchor_lang::{prelude::Pubkey, system_program};
use solana_instruction::Instruction;
use tekmerion::{Policy, PolicyRules};

use super::instruction;

/// Explicit administrative inputs; this builder does not sign or submit.
pub struct CreatePolicy {
    pub payer: Pubkey,
    pub admin: Pubkey,

    pub client_authority: Pubkey,
    pub identifier: u64,
    pub rules: PolicyRules,
}

pub fn create_policy(input: CreatePolicy) -> Instruction {
    instruction(
        tekmerion::accounts::CreatePolicy {
            payer: input.payer,
            admin: input.admin,

            policy: Policy::address(&input.admin, input.identifier).0,

            system_program: system_program::ID,
        },
        tekmerion::instruction::CreatePolicy {
            identifier: input.identifier,
            client_authority: input.client_authority,
            rules: input.rules,
        },
    )
}

pub fn update_policy(admin: Pubkey, identifier: u64, rules: PolicyRules) -> Instruction {
    instruction(
        tekmerion::accounts::UpdatePolicy {
            admin,

            policy: Policy::address(&admin, identifier).0,
        },
        tekmerion::instruction::UpdatePolicy { rules },
    )
}

/// Retain a client dependency using the exact authority configured by Policy.
/// The client owns acquisition timing and duplicate-acquisition prevention.
pub fn acquire_policy(policy: Pubkey, client_authority: Pubkey, subject: Pubkey) -> Instruction {
    instruction(
        tekmerion::accounts::AcquirePolicy {
            client_authority,

            policy,
            subject,
        },
        tekmerion::instruction::AcquirePolicy {},
    )
}
