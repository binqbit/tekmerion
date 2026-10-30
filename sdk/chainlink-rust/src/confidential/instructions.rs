//! Transaction builders; account order is defined by the program's Anchor types.
use super::{ConfidentialConfig, ConfidentialProfile, ConfidentialRequest};
use crate::{CaseAddress, instruction, program};
use anchor_lang::{prelude::*, system_program};
use solana_instruction::{AccountMeta, Instruction};

/// Includes native Forwarder signature verification and authenticated delivery.
pub const COMPUTE_UNITS: u32 = 400_000;
pub fn compute_budget() -> Instruction {
    Instruction {
        program_id: Pubkey::from_str_const("ComputeBudget111111111111111111111111111111"),
        accounts: Vec::new(),
        data: [&[2][..], &COMPUTE_UNITS.to_le_bytes()].concat(),
    }
}

/// Create the core Policy and its immutable auditor configuration atomically.
pub fn create_policy(
    input: tekmerion_sdk::instructions::CreatePolicy,
    config: ConfidentialConfig,
) -> Vec<Instruction> {
    let registration = register_profile(
        input.payer,
        input.admin,
        tekmerion_sdk::program::Policy::address(&input.admin, input.identifier).0,
        config,
    );
    vec![
        tekmerion_sdk::instructions::create_policy(input),
        registration,
    ]
}

pub fn register_profile(
    payer: Pubkey,
    admin: Pubkey,
    policy: Pubkey,
    config: ConfidentialConfig,
) -> Instruction {
    instruction(
        program::accounts::RegisterConfidentialProfile {
            payer,
            admin,
            policy,
            profile: ConfidentialProfile::address(&policy).0,
            forwarder_program: config.forwarder_program,
            forwarder_state: config.forwarder_state,
            system_program: system_program::ID,
        },
        program::instruction::RegisterConfidentialProfile { config },
    )
}

pub fn prepare_request(payer: Pubkey, case: CaseAddress) -> Instruction {
    let profile = ConfidentialProfile::address(&case.policy).0;
    instruction(
        program::accounts::PrepareConfidentialRequest {
            payer,
            profile,
            policy: case.policy,
            case: case.address(),
            request: ConfidentialRequest::address(&profile, &case.address()).0,
            system_program: system_program::ID,
        },
        program::instruction::PrepareConfidentialRequest {},
    )
}

pub fn apply_verdict(case: CaseAddress, fee_destination: Pubkey) -> Instruction {
    let profile = ConfidentialProfile::address(&case.policy).0;
    instruction(
        program::accounts::ApplyConfidentialVerdict {
            profile,
            policy: case.policy,
            case: case.address(),
            request: ConfidentialRequest::address(&profile, &case.address()).0,
            mint: tekmerion_sdk::USDC_MINT,
            vault: case.vault(),
            token_program: anchor_spl::token::ID,
            tekmerion_program: tekmerion_sdk::ID,
            fee_destination,
        },
        program::instruction::ApplyConfidentialVerdict {},
    )
}
/// Exact receiver list committed by the native Forwarder account hash.
pub fn receiver_accounts(case: CaseAddress, config: &ConfidentialConfig) -> Vec<AccountMeta> {
    let profile = ConfidentialProfile::address(&case.policy).0;
    let request = ConfidentialRequest::address(&profile, &case.address()).0;
    program::accounts::OnReport {
        forwarder_state: config.forwarder_state,
        forwarder_authority: program::report::authority(config),
        profile,
        policy: case.policy,
        case: case.address(),
        request,
    }
    .to_account_metas(None)
}
