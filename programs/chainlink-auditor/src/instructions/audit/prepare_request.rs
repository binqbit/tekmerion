use anchor_lang::prelude::*;
use tekmerion_sdk::program::{Case, Policy};

use crate::{
    confidential::{self, binding},
    errors::AdapterError,
    state::{ConfidentialProfile, ConfidentialRequest},
};

#[derive(Accounts)]
pub struct PrepareConfidentialRequest<'info> {
    // Transaction funding and authority.
    #[account(mut)]
    pub payer: Signer<'info>,

    // Canonical policy and audit context.
    #[account(
        seeds = [ConfidentialProfile::SEED, policy.key().as_ref()],
        bump = profile.bump,
    )]
    pub profile: Box<Account<'info, ConfidentialProfile>>,
    pub policy: Box<Account<'info, Policy>>,
    pub case: Box<Account<'info, Case>>,
    // One committed request per profile and case.
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + ConfidentialRequest::INIT_SPACE,
        seeds = [
            ConfidentialRequest::SEED,
            profile.key().as_ref(),
            case.key().as_ref(),
        ],
        bump,
    )]
    pub request: Box<Account<'info, ConfidentialRequest>>,

    // Programs.
    pub system_program: Program<'info, System>,
}

pub(crate) fn handle(ctx: Context<PrepareConfidentialRequest>) -> Result<()> {
    let profile = &ctx.accounts.profile;
    binding::validate_profile(
        profile.key(),
        profile,
        ctx.accounts.policy.key(),
        &ctx.accounts.policy,
    )?;
    let digest = binding::request_digest(
        profile.key(),
        profile,
        ctx.accounts.case.key(),
        &ctx.accounts.case,
    )?;
    // An existing request must retain exactly the same commitments.
    let request = &mut ctx.accounts.request;
    if request.version != 0 {
        require!(
            request.version == confidential::VERSION
                && request.profile == profile.key()
                && request.case == ctx.accounts.case.key()
                && request.digest == digest,
            AdapterError::InvalidBinding
        );
        return Ok(());
    }

    // Initialize only while the core audit is open.
    crate::binding::require_open(&ctx.accounts.case, Clock::get()?.unix_timestamp)?;
    request.set_inner(ConfidentialRequest::prepared(
        profile.key(),
        ctx.accounts.case.key(),
        digest,
        ctx.bumps.request,
    ));
    Ok(())
}
