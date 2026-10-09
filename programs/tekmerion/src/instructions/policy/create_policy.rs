use anchor_lang::prelude::*;

use crate::{Policy, PolicyRules};

#[derive(Accounts)]
#[instruction(identifier: u64)]
pub struct CreatePolicy<'info> {
    // Signers.
    #[account(mut)]
    pub payer: Signer<'info>,
    pub admin: Signer<'info>,

    // Protocol accounts.
    #[account(
        init,
        payer = payer,
        space = Policy::SPACE,
        seeds = [Policy::SEED, admin.key().as_ref(), &identifier.to_le_bytes()],
        bump,
    )]
    pub policy: Account<'info, Policy>,

    // Programs.
    pub system_program: Program<'info, System>,
}

pub(crate) fn handle(
    ctx: Context<CreatePolicy>,
    identifier: u64,
    client_authority: Pubkey,
    rules: PolicyRules,
) -> Result<()> {
    rules.validate_at(Clock::get()?.unix_timestamp)?;

    ctx.accounts.policy.set_inner(Policy {
        admin: ctx.accounts.admin.key(),
        identifier,
        client_authority,
        rules,
        active_uses: 0,
        bump: ctx.bumps.policy,
    });

    Ok(())
}
