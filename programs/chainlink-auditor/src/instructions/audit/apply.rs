use crate::{
    confidential::{self, binding},
    errors::AdapterError,
    state::{ConfidentialProfile, ConfidentialRequest, ConfidentialState},
};
use anchor_lang::prelude::*;
use tekmerion_sdk::program::{AuditDecision, Case, Policy, program::Tekmerion};

#[derive(Accounts)]
pub struct ApplyConfidentialVerdict<'info> {
    // Committed audit context.
    #[account(
        seeds = [ConfidentialProfile::SEED, policy.key().as_ref()],
        bump = profile.bump,
    )]
    pub profile: Box<Account<'info, ConfidentialProfile>>,
    pub policy: Box<Account<'info, Policy>>,
    #[account(mut)]
    pub case: Box<Account<'info, Case>>,
    #[account(
        mut,
        seeds = [
            ConfidentialRequest::SEED,
            profile.key().as_ref(),
            case.key().as_ref(),
        ],
        bump = request.bump,
        has_one = profile,
        has_one = case,
    )]
    pub request: Box<Account<'info, ConfidentialRequest>>,
    // Token custody, validated by the core program.
    /// CHECK: core validates the collateral mint.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: core validates vault and backing.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,

    // Core CPI programs.
    /// CHECK: core validates SPL program.
    pub token_program: UncheckedAccount<'info>,
    pub tekmerion_program: Program<'info, Tekmerion>,

    // The Policy auditor's payout account.
    /// CHECK: core authenticates the single Policy auditor's SPL fee destination.
    #[account(mut)]
    pub fee_destination: UncheckedAccount<'info>,
}

pub(crate) fn handle<'info>(ctx: Context<'info, ApplyConfidentialVerdict<'info>>) -> Result<()> {
    require!(
        ctx.remaining_accounts.is_empty(),
        AdapterError::InvalidPayload
    );
    ctx.accounts.validate_request()?;

    // Reapplying an already applied request is a no-op, never a second fee.
    let now = Clock::get()?.unix_timestamp;
    let Some(decision) = ctx.accounts.request.decision(&ctx.accounts.case, now)? else {
        return Ok(());
    };

    // Core vote, fee payment and Applied state remain one atomic transaction.
    ctx.accounts.submit_decision(decision)?;
    ctx.accounts.case.reload()?;
    ctx.accounts.request.state = ConfidentialState::Applied;
    ctx.accounts.request.applied_at = now;
    Ok(())
}

impl<'info> ApplyConfidentialVerdict<'info> {
    fn validate_request(&self) -> Result<()> {
        let profile = &self.profile;
        binding::validate_profile(profile.key(), profile, self.policy.key(), &self.policy)?;
        require!(
            self.request.version == confidential::VERSION
                && self.request.digest
                    == binding::request_digest(
                        profile.key(),
                        profile,
                        self.case.key(),
                        &self.case
                    )?,
            AdapterError::InvalidBinding
        );
        Ok(())
    }

    fn submit_decision(&self, decision: AuditDecision) -> Result<()> {
        let profile = &self.profile;
        let bump = [profile.bump];
        let seeds = [ConfidentialProfile::SEED, profile.policy.as_ref(), &bump];
        let signers = [&seeds[..]];
        let accounts = tekmerion_sdk::cpi::accounts::SubmitAudit {
            auditor: profile.to_account_info(),
            case: self.case.to_account_info(),
            policy: self.policy.to_account_info(),
            mint: self.mint.to_account_info(),
            vault: self.vault.to_account_info(),
            token_program: self.token_program.to_account_info(),
        };
        let cpi = CpiContext::new_with_signer(self.tekmerion_program.key(), accounts, &signers)
            .with_remaining_accounts(vec![self.fee_destination.to_account_info()]);
        tekmerion_sdk::cpi::submit_audit(cpi, decision.answer, decision.reason)
    }
}
