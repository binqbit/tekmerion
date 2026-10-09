use anchor_lang::prelude::*;

use crate::errors::TekmerionError;
use crate::{Policy, PolicyRules};

#[derive(Accounts)]
pub struct UpdatePolicy<'info> {
    // Signers.
    pub admin: Signer<'info>,

    // Protocol accounts.
    #[account(
        mut,
        has_one = admin,
        seeds = [
            Policy::SEED,
            policy.admin.as_ref(),
            &policy.identifier.to_le_bytes(),
        ],
        bump = policy.bump,
    )]
    pub policy: Account<'info, Policy>,
}

pub(crate) fn handle(ctx: Context<UpdatePolicy>, rules: PolicyRules) -> Result<()> {
    require!(
        ctx.accounts.policy.active_uses == 0,
        TekmerionError::PolicyInUse
    );
    rules.validate_at(Clock::get()?.unix_timestamp)?;

    ctx.accounts.policy.rules = rules;

    Ok(())
}
