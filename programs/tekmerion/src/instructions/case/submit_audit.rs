use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    AuditAnswer, AuditDecision, AuditReason, AuditVote, Case, Policy, USDC_MINT,
    errors::TekmerionError,
};

#[derive(Accounts)]
pub struct SubmitAudit<'info> {
    // Signers.
    pub auditor: Signer<'info>,

    // Protocol accounts.
    #[account(
        mut,
        seeds = [Case::SEED, case.policy.as_ref(), case.subject.as_ref(), &case.nonce.to_le_bytes()],
        bump = case.bump,
    )]
    pub case: Box<Account<'info, Case>>,
    #[account(
        address = case.policy,
        seeds = [Policy::SEED, policy.admin.as_ref(), &policy.identifier.to_le_bytes()],
        bump = policy.bump,
    )]
    pub policy: Box<Account<'info, Policy>>,

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

    // Programs.
    pub token_program: Program<'info, Token>,
}

pub(crate) fn handle<'info>(
    ctx: Context<'info, SubmitAudit<'info>>,
    answer: AuditAnswer,
    reason: AuditReason,
) -> Result<()> {
    let auditor_index = ctx
        .accounts
        .policy
        .rules
        .auditors
        .iter()
        .position(|auditor| auditor.authority == ctx.accounts.auditor.key())
        .ok_or(TekmerionError::UnauthorizedAuditor)?;
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts.case.require_audit_open(now)?;
    let decision = AuditDecision::new(answer, reason)?;

    // Record the authorized vote. A vote below quorum does not settle the Case.
    ctx.accounts.case.audit.record_vote(AuditVote {
        auditor_index: auditor_index as u8,
        response: decision,
    })?;

    let winners = ctx.accounts.case.audit.matching_auditors(decision.answer);
    let winner_count = winners.len();
    if winner_count < usize::from(ctx.accounts.policy.rules.threshold) {
        require!(
            ctx.remaining_accounts.is_empty(),
            TekmerionError::InvalidPayoutAccounts
        );
        return Ok(());
    }

    require!(
        ctx.remaining_accounts.len() == winner_count,
        TekmerionError::InvalidPayoutAccounts
    );

    let total_fee = ctx.accounts.policy.rules.audit_fee;
    let allocation = ctx
        .accounts
        .case
        .audited_allocation(decision.answer, total_fee)?;

    crate::custody::require_valid_vault(&ctx.accounts.vault, &ctx.accounts.case.key())?;
    crate::custody::require_backing(ctx.accounts.vault.amount, allocation.total().base_units())?;

    // Finalization and every fee transfer are atomic: a payout error also rolls
    // back the vote and the entitlements recorded here.
    ctx.accounts
        .case
        .finalize_audited(decision, allocation, now)?;

    let base_fee_share = total_fee / winner_count as u64;
    let remainder_recipients = (total_fee % winner_count as u64) as usize;

    let nonce = ctx.accounts.case.nonce.to_le_bytes();
    let bump = [ctx.accounts.case.bump];
    let seeds = ctx.accounts.case.signer_seeds(&nonce, &bump);

    // Winners are in policy order; the first remainder recipients receive one
    // additional base unit so fee shares differ by at most one.
    for (position, (auditor_index, info)) in winners.iter().zip(ctx.remaining_accounts).enumerate()
    {
        require!(info.is_writable, TekmerionError::InvalidPayoutAccounts);
        let mut destination = Account::<TokenAccount>::try_from(info)?;
        require_keys_eq!(
            destination.owner,
            ctx.accounts.policy.rules.auditors[usize::from(*auditor_index)].fee_recipient,
            TekmerionError::InvalidFeeRecipient
        );

        let fee_share = if position < remainder_recipients {
            base_fee_share + 1
        } else {
            base_fee_share
        };

        crate::custody::transfer(
            &mut ctx.accounts.vault,
            &mut destination,
            &ctx.accounts.mint,
            ctx.accounts.case.to_account_info(),
            &ctx.accounts.token_program,
            fee_share,
            &[&seeds],
        )?;
    }

    ctx.accounts.case.fee_claimed = true;

    // Author claims cannot precede this single terminal transition.
    crate::custody::require_backing(
        ctx.accounts.vault.amount,
        ctx.accounts.case.outstanding_liability(false),
    )?;

    Ok(())
}
