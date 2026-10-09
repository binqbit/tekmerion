use anchor_lang::prelude::*;

use crate::{Policy, errors::TekmerionError};

#[derive(Accounts)]
pub struct AcquirePolicy<'info> {
    // Signers.
    #[account(address = policy.client_authority)]
    pub client_authority: Signer<'info>,

    // Protocol and subject accounts.
    #[account(
        mut,
        seeds = [Policy::SEED, policy.admin.as_ref(), &policy.identifier.to_le_bytes()],
        bump = policy.bump,
    )]
    pub policy: Account<'info, Policy>,
    /// CHECK: opaque client identity; the configured signer authorizes the
    /// dependency and the handler rejects executable accounts.
    pub subject: UncheckedAccount<'info>,
}

pub(crate) fn handle(ctx: Context<AcquirePolicy>) -> Result<()> {
    require!(
        ctx.remaining_accounts.is_empty(),
        TekmerionError::InvalidSubject
    );
    require!(
        !ctx.accounts.subject.executable,
        TekmerionError::InvalidSubject
    );

    // The configured client acquires its dependency as part of its own atomic
    // lifecycle transition. Case creation retains a separate use for audit claims.
    ctx.accounts.policy.active_uses = ctx
        .accounts
        .policy
        .active_uses
        .checked_add(1)
        .ok_or(TekmerionError::ArithmeticOverflow)?;

    Ok(())
}
