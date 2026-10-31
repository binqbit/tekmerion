use crate::{
    binding, confidential,
    errors::AdapterError,
    state::{ConfidentialConfig, ConfidentialProfile},
};
use anchor_lang::prelude::*;
use tekmerion_sdk::program::Policy;

#[derive(Accounts)]
#[instruction(config: ConfidentialConfig)]
pub struct RegisterConfidentialProfile<'info> {
    // Transaction funding and authority.
    #[account(mut)]
    pub payer: Signer<'info>,
    pub admin: Signer<'info>,

    // Policy and its immutable auditor profile.
    #[account(
        seeds = [
            Policy::SEED,
            policy.admin.as_ref(),
            &policy.identifier.to_le_bytes(),
        ],
        bump = policy.bump,
        seeds::program = tekmerion_sdk::ID,
        has_one = admin,
    )]
    pub policy: Box<Account<'info, Policy>>,
    #[account(
        init,
        payer = payer,
        space = 8 + ConfidentialProfile::INIT_SPACE,
        seeds = [ConfidentialProfile::SEED, policy.key().as_ref()],
        bump,
    )]
    pub profile: Box<Account<'info, ConfidentialProfile>>,
    // Delivery identity chosen by this Policy.
    /// CHECK: executable address must match the declared configuration.
    #[account(executable, address = config.forwarder_program)]
    pub forwarder_program: UncheckedAccount<'info>,
    /// CHECK: the declared state must belong to the declared Forwarder program.
    #[account(
        address = config.forwarder_state,
        owner = config.forwarder_program,
    )]
    pub forwarder_state: UncheckedAccount<'info>,

    // Programs.
    pub system_program: Program<'info, System>,
}

pub(crate) fn handle(
    ctx: Context<RegisterConfidentialProfile>,
    config: ConfidentialConfig,
) -> Result<()> {
    let policy = &ctx.accounts.policy;
    require!(
        policy.active_uses == 0
            && policy.rules.auditors.len() == 1
            && policy.rules.threshold == 1
            && policy.rules.has_valid_parameters()
            && policy.client_authority != Pubkey::default()
            && policy.rules.auditors[0].authority == ctx.accounts.profile.key(),
        AdapterError::InvalidProfile
    );

    // Pin the policy rules and deployment identity once.
    ctx.accounts.profile.set_inner(ConfidentialProfile {
        version: confidential::VERSION,
        bump: ctx.bumps.profile,
        policy: policy.key(),
        rules_hash: binding::rules_hash(policy)?,
        config,
    });
    ctx.accounts.profile.validate_config()
}
