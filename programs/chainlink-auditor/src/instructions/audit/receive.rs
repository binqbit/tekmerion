//! Authenticate a native Chainlink delivery before advancing its bound audit task.
use crate::{
    confidential::{self, binding, payload::AuditReport},
    errors::AdapterError,
    report,
    state::{ConfidentialProfile, ConfidentialRequest, ReceiveContext},
};
use anchor_lang::prelude::*;
use tekmerion_sdk::program::{Case, Policy};

#[derive(Accounts)]
pub struct OnReport<'info> {
    // Authenticated delivery.
    /// CHECK: state owner/key and the scoped Forwarder signer are authenticated below.
    pub forwarder_state: UncheckedAccount<'info>,
    pub forwarder_authority: Signer<'info>,

    // Committed audit context.
    #[account(seeds = [ConfidentialProfile::SEED, policy.key().as_ref()], bump = profile.bump)]
    pub profile: Box<Account<'info, ConfidentialProfile>>,
    pub policy: Box<Account<'info, Policy>>,
    pub case: Box<Account<'info, Case>>,
    #[account(
        mut,
        seeds = [ConfidentialRequest::SEED, profile.key().as_ref(), case.key().as_ref()],
        bump = request.bump, has_one = profile, has_one = case,
    )]
    pub request: Box<Account<'info, ConfidentialRequest>>,
}

pub(crate) fn handle(ctx: Context<OnReport>, metadata: &[u8], bytes: &[u8]) -> Result<()> {
    require!(
        ctx.remaining_accounts.is_empty(),
        AdapterError::InvalidPayload
    );
    let payload = AuditReport::decode(bytes)?;
    ctx.accounts
        .authenticate_delivery(metadata, payload.prefix.stage)?;
    ctx.accounts.validate_request()?;
    ctx.accounts.request.receive(
        &payload,
        ReceiveContext {
            profile: &ctx.accounts.profile,
            case: &ctx.accounts.case,
            now: Clock::get()?.unix_timestamp,
        },
    )
}

impl OnReport<'_> {
    fn authenticate_delivery(&self, metadata: &[u8], stage: u8) -> Result<()> {
        report::authenticate(
            &self.profile.config,
            stage,
            &self.forwarder_state.to_account_info(),
            self.forwarder_authority.key(),
            metadata,
        )
    }

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
}
