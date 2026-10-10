//! Test-only consumer: SDK CPI calls authorized by a controller's PDA.

use anchor_lang::prelude::*;
use tekmerion_sdk::program::{AuditAnswer, AuditReason, program::Tekmerion};

// Fixed fixture identity, derived from public [0xc7; 32] bytes, not a deployment key.
declare_id!("ESrpUvg2gM75m1mzoSuquCaoabs42edBCCdabdvDgJBg");

pub const AUTHORITY_SEED: &[u8] = b"authority";

#[program]
pub mod sdk_consumer {
    use super::*;

    pub fn create_case<'info>(
        ctx: Context<'info, CreateCase<'info>>,
        nonce: u64,
        author_principal: u64,
        agreement_hash: [u8; 32],
        result_hash: [u8; 32],
    ) -> Result<()> {
        let controller = ctx.accounts.controller.key();
        let bump = [ctx.bumps.authority];
        let seeds = [AUTHORITY_SEED, controller.as_ref(), bump.as_ref()];
        let signer_seeds = [&seeds[..]];
        let accounts = tekmerion_sdk::cpi::accounts::CreateCase {
            payer: ctx.accounts.payer.to_account_info(),
            client_authority: ctx.accounts.authority.to_account_info(),
            author: ctx.accounts.author.to_account_info(),
            policy: ctx.accounts.policy.to_account_info(),
            subject: ctx.accounts.subject.to_account_info(),
            case: ctx.accounts.case.to_account_info(),
            author_receipt: ctx.accounts.author_receipt.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            source: ctx.accounts.source.to_account_info(),
            vault: ctx.accounts.vault.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        let cpi = CpiContext::new_with_signer(
            ctx.accounts.tekmerion_program.key(),
            accounts,
            &signer_seeds,
        )
        .with_remaining_accounts(ctx.remaining_accounts.to_vec());

        tekmerion_sdk::cpi::create_case(cpi, nonce, author_principal, agreement_hash, result_hash)
    }

    pub fn submit_audit<'info>(
        ctx: Context<'info, SubmitAudit<'info>>,
        answer: AuditAnswer,
        reason: AuditReason,
    ) -> Result<()> {
        let controller = ctx.accounts.controller.key();
        let bump = [ctx.bumps.authority];
        let seeds = [AUTHORITY_SEED, controller.as_ref(), bump.as_ref()];
        let signer_seeds = [&seeds[..]];
        let accounts = tekmerion_sdk::cpi::accounts::SubmitAudit {
            auditor: ctx.accounts.authority.to_account_info(),
            case: ctx.accounts.case.to_account_info(),
            policy: ctx.accounts.policy.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            vault: ctx.accounts.vault.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
        };
        let cpi = CpiContext::new_with_signer(
            ctx.accounts.tekmerion_program.key(),
            accounts,
            &signer_seeds,
        )
        .with_remaining_accounts(ctx.remaining_accounts.to_vec());

        tekmerion_sdk::cpi::submit_audit(cpi, answer, reason)
    }
}

#[derive(Accounts)]
pub struct CreateCase<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub controller: Signer<'info>,
    pub author: Signer<'info>,
    /// CHECK: controller-bound PDA; invoke_signed grants its inner signature.
    #[account(seeds = [AUTHORITY_SEED, controller.key().as_ref()], bump)]
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates its canonical Policy and creation authority.
    #[account(mut)]
    pub policy: UncheckedAccount<'info>,
    /// CHECK: opaque fixture identity; Tekmerion validates its attempt history.
    #[account(mut)]
    pub subject: UncheckedAccount<'info>,
    /// CHECK: Tekmerion initializes its canonical Case PDA during CPI.
    #[account(mut)]
    pub case: UncheckedAccount<'info>,
    /// CHECK: Tekmerion initializes the author's canonical receipt during CPI.
    #[account(mut)]
    pub author_receipt: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates the selected USDC mint.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Tekmerion validates the author's token account and bond transfer.
    #[account(mut)]
    pub source: UncheckedAccount<'info>,
    /// CHECK: Tekmerion initializes its canonical token vault during CPI.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates the SPL Token program.
    pub token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub tekmerion_program: Program<'info, Tekmerion>,
}

#[derive(Accounts)]
pub struct SubmitAudit<'info> {
    pub controller: Signer<'info>,
    /// CHECK: controller-bound PDA; invoke_signed grants its inner signature.
    #[account(seeds = [AUTHORITY_SEED, controller.key().as_ref()], bump)]
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates and updates its own Case during CPI.
    #[account(mut)]
    pub case: UncheckedAccount<'info>,
    /// CHECK: Tekmerion authenticates this Policy and its auditor committee.
    pub policy: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates the selected USDC mint.
    pub mint: UncheckedAccount<'info>,
    /// CHECK: Tekmerion validates custody and signs quorum fee transfers.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,

    /// CHECK: Tekmerion validates the SPL Token program.
    pub token_program: UncheckedAccount<'info>,
    pub tekmerion_program: Program<'info, Tekmerion>,
}
