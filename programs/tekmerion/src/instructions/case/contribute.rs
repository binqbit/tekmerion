use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::calculations::{amounts::UsdcAmount, time::ChainTimestamp};
use crate::{Case, Policy, USDC_MINT, errors::TekmerionError};
use crate::{StakeReceipt, errors::domain_error};

#[derive(Accounts)]
pub struct Contribute<'info> {
    // Signers.
    #[account(mut)]
    pub payer: Signer<'info>,
    pub owner: Signer<'info>,

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
    pub case: Box<Account<'info, Case>>,
    #[account(
        address = case.policy,
        seeds = [
            Policy::SEED,
            policy.admin.as_ref(),
            &policy.identifier.to_le_bytes(),
        ],
        bump = policy.bump,
    )]
    pub policy: Box<Account<'info, Policy>>,
    #[account(
        init_if_needed,
        payer = payer,
        space = StakeReceipt::SPACE,
        seeds = [StakeReceipt::SEED, case.key().as_ref(), owner.key().as_ref()],
        bump,
    )]
    pub receipt: Account<'info, StakeReceipt>,

    // Token custody.
    #[account(address = USDC_MINT)]
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = owner,
    )]
    pub source: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [Case::VAULT_SEED, case.key().as_ref()],
        bump = case.vault_bump,
        token::mint = mint,
        token::authority = case,
    )]
    pub vault: Account<'info, TokenAccount>,

    // Programs.
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub(crate) fn handle(ctx: Context<Contribute>, requested: u64) -> Result<()> {
    ctx.accounts.case.require_funding()?;
    require!(requested > 0, TekmerionError::InvalidContribution);
    require_keys_neq!(
        ctx.accounts.owner.key(),
        ctx.accounts.case.author,
        TekmerionError::UnsupportedRole
    );

    let now = ChainTimestamp::from_unix_seconds(Clock::get()?.unix_timestamp);
    let quote = ctx
        .accounts
        .case
        .funding_snapshot()?
        .quote_contribution(UsdcAmount::from_base_units(requested), now)
        .map_err(domain_error)?;

    let previous_receipt_principal = ctx.accounts.receipt.principal;
    let accepted = quote.accepted().base_units();
    let updated_receipt = ctx.accounts.receipt.prepare_contribution(
        ctx.accounts.case.key(),
        ctx.accounts.owner.key(),
        ctx.bumps.receipt,
        accepted,
    )?;

    crate::custody::require_valid_vault(&ctx.accounts.vault, &ctx.accounts.case.key())?;
    let existing_liability = u128::from(ctx.accounts.case.author_principal)
        + u128::from(ctx.accounts.case.challenge_principal);
    crate::custody::require_backing(ctx.accounts.vault.amount, existing_liability)?;

    crate::custody::transfer(
        &mut ctx.accounts.source,
        &mut ctx.accounts.vault,
        &ctx.accounts.mint,
        ctx.accounts.owner.to_account_info(),
        &ctx.accounts.token_program,
        accepted,
        &[],
    )?;

    ctx.accounts.receipt.set_inner(updated_receipt);
    // Reaching the target also starts the audit deadline. Any error here rolls
    // back the transfer and receipt update together with the transaction.
    ctx.accounts.case.record_contribution(
        quote,
        previous_receipt_principal,
        &ctx.accounts.policy.rules,
        now,
    )
}
