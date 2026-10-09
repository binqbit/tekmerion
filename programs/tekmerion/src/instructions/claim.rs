use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::custody::pay;
use crate::{AUTHOR_SIDE, CHALLENGE_SIDE, Case, StakeReceipt, USDC_MINT, errors::TekmerionError};

#[derive(Accounts)]
pub struct Claim<'info> {
    // Signers.
    pub owner: Signer<'info>,

    // Protocol accounts.
    #[account(
        mut,
        seeds = [
            Case::SEED,
            case.policy.as_ref(),
            case.subject.as_ref(),
            &case.nonce.to_le_bytes(),
        ],
        bump = case.bump,
    )]
    pub case: Box<Account<'info, Case>>,
    #[account(
        has_one = case,
        constraint = author_receipt.owner == case.author @ TekmerionError::InvalidReceipt,
        seeds = [StakeReceipt::SEED, case.key().as_ref(), case.author.as_ref()],
        bump = author_receipt.bump,
    )]
    pub author_receipt: Box<Account<'info, StakeReceipt>>,
    #[account(
        mut,
        has_one = case,
        has_one = owner,
        seeds = [StakeReceipt::SEED, case.key().as_ref(), owner.key().as_ref()],
        bump = receipt.bump,
    )]
    pub receipt: Account<'info, StakeReceipt>,

    // Token custody.
    #[account(address = USDC_MINT)]
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [Case::VAULT_SEED, case.key().as_ref()],
        bump = case.vault_bump,
        token::mint = mint,
        token::authority = case,
    )]
    pub vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = owner,
    )]
    pub destination: Account<'info, TokenAccount>,

    // Programs.
    pub token_program: Program<'info, Token>,
}

pub(crate) fn handle(ctx: Context<Claim>) -> Result<()> {
    ctx.accounts.case.require_finalized()?;
    ctx.accounts
        .author_receipt
        .require_author_principal(ctx.accounts.case.author_principal)?;

    let is_author = ctx.accounts.owner.key() == ctx.accounts.case.author;
    let claim_side = if is_author {
        AUTHOR_SIDE
    } else {
        CHALLENGE_SIDE
    };
    ctx.accounts.receipt.require_claimable(claim_side)?;

    let (amount, contributor_claim) = if is_author {
        (ctx.accounts.case.author_entitlement, None)
    } else {
        let claim = ctx
            .accounts
            .case
            .prepare_contributor_claim(ctx.accounts.receipt.principal)?;
        (claim.amount(), Some(claim))
    };
    let author_claimed_before_payment = ctx.accounts.author_receipt.claimed;
    let outstanding_liability = ctx
        .accounts
        .case
        .outstanding_liability(author_claimed_before_payment);

    pay(
        &ctx.accounts.case,
        &mut ctx.accounts.vault,
        &mut ctx.accounts.destination,
        &ctx.accounts.mint,
        &ctx.accounts.token_program,
        amount,
        outstanding_liability,
    )?;

    ctx.accounts.receipt.claimed = true;
    if let Some(claim) = contributor_claim {
        ctx.accounts.case.record_contributor_claim(claim);
    }

    // The author claim aliases both receipt accounts. Only the mutable receipt
    // is saved on exit; its readonly counterpart still holds the initial value.
    let author_claimed_after_payment = is_author || author_claimed_before_payment;
    crate::custody::require_backing(
        ctx.accounts.vault.amount,
        ctx.accounts
            .case
            .outstanding_liability(author_claimed_after_payment),
    )?;

    Ok(())
}
