use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;

use crate::{Case, USDC_MINT, errors::TekmerionError};

#[derive(Accounts)]
pub struct FinalizeTimeout<'info> {
    // Protocol accounts.
    #[account(
        mut,
        seeds = [
            Case::SEED,
            case.policy.as_ref(),
            case.subject.as_ref(),
            case.author.as_ref(),
            &case.nonce.to_le_bytes(),
        ],
        bump = case.bump,
    )]
    pub case: Account<'info, Case>,

    // Token custody.
    #[account(
        seeds = [Case::VAULT_SEED, case.key().as_ref()],
        bump = case.vault_bump,
        constraint = vault.mint == USDC_MINT @ TekmerionError::InvalidTokenAccount,
        token::authority = case,
    )]
    pub vault: Account<'info, TokenAccount>,
}

pub(crate) fn handle(ctx: Context<FinalizeTimeout>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let allocation = ctx.accounts.case.timeout_allocation(now)?;

    crate::custody::require_valid_vault(&ctx.accounts.vault, &ctx.accounts.case.key())?;
    crate::custody::require_backing(ctx.accounts.vault.amount, allocation.total().base_units())?;

    ctx.accounts.case.finalize_timeout(allocation, now)
}
