use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::calculations::time::{ChainDeadline, ChainTimestamp};
use crate::state::case::CaseInitialization;
use crate::{
    Case, CaseFunding, CaseMode, CasePhase, CaseResult, CompletionMethod, Policy, StakeReceipt,
    USDC_MINT, errors::TekmerionError,
};

#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct CreateCase<'info> {
    // Signers.
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = policy.client_authority)]
    pub client_authority: Signer<'info>,
    pub author: Signer<'info>,

    // Protocol and subject accounts.
    #[account(
        mut,
        seeds = [
            Policy::SEED,
            policy.admin.as_ref(),
            &policy.identifier.to_le_bytes(),
        ],
        bump = policy.bump,
    )]
    pub policy: Box<Account<'info, Policy>>,
    /// CHECK: opaque client identity authorized by Policy's configured signer;
    /// the handler rejects executables and validates Tekmerion's Case history.
    #[account(mut)]
    pub subject: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        space = Case::SPACE,
        seeds = [
            Case::SEED,
            policy.key().as_ref(),
            subject.key().as_ref(),
            author.key().as_ref(),
            &nonce.to_le_bytes(),
        ],
        bump,
    )]
    pub case: Box<Account<'info, Case>>,
    #[account(
        init,
        payer = payer,
        space = StakeReceipt::SPACE,
        seeds = [StakeReceipt::SEED, case.key().as_ref(), author.key().as_ref()],
        bump,
    )]
    pub author_receipt: Box<Account<'info, StakeReceipt>>,

    // Token custody.
    #[account(address = USDC_MINT)]
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = author,
    )]
    pub source: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = payer,
        seeds = [Case::VAULT_SEED, case.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = case,
    )]
    pub vault: Account<'info, TokenAccount>,

    // Programs.
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub(crate) fn handle(
    ctx: Context<CreateCase>,
    nonce: u64,
    funding: CaseFunding,
    agreement_hash: [u8; 32],
    result_hash: [u8; 32],
) -> Result<()> {
    require!(
        agreement_hash != [0; 32] && result_hash != [0; 32],
        TekmerionError::InvalidCommitment
    );
    let previous_case = validate_previous_case(
        &ctx.accounts.subject.to_account_info(),
        &ctx.accounts.policy.key(),
        &ctx.accounts.author.key(),
        nonce,
        ctx.remaining_accounts,
    )?;

    let now = ChainTimestamp::from_unix_seconds(Clock::get()?.unix_timestamp);
    let rules = &ctx.accounts.policy.rules;
    let (mode, author_principal, challenge_target, funding_deadline, audit_deadline) = match funding
    {
        CaseFunding::Bonded { principal } => {
            require!(
                principal >= rules.author_bond,
                TekmerionError::InsufficientBond
            );
            principal
                .checked_add(rules.challenge_target)
                .ok_or(TekmerionError::ArithmeticOverflow)?;
            let deadline = ChainDeadline::checked_after(now, rules.funding_duration)
                .map_err(crate::errors::domain_error)?;
            (
                CaseMode::Challenge,
                principal,
                rules.challenge_target,
                deadline.unix_seconds(),
                None,
            )
        }
        CaseFunding::DirectAudit { expected_fee } => {
            require!(
                expected_fee == rules.audit_fee,
                TekmerionError::InvalidAuditFee
            );
            let deadline = ChainDeadline::checked_after(now, rules.audit_timeout)
                .map_err(crate::errors::domain_error)?;
            (
                CaseMode::DirectAudit,
                rules.audit_fee,
                0,
                now.unix_seconds(),
                Some(deadline.unix_seconds()),
            )
        }
    };
    let active_uses = ctx
        .accounts
        .policy
        .active_uses
        .checked_add(1)
        .ok_or(TekmerionError::ArithmeticOverflow)?;

    crate::custody::require_valid_vault(&ctx.accounts.vault, &ctx.accounts.case.key())?;

    crate::custody::transfer(
        &mut ctx.accounts.source,
        &mut ctx.accounts.vault,
        &ctx.accounts.mint,
        ctx.accounts.author.to_account_info(),
        &ctx.accounts.token_program,
        author_principal,
        &[],
    )?;

    ctx.accounts.case.set_inner(Case::new(CaseInitialization {
        policy: ctx.accounts.policy.key(),
        subject: ctx.accounts.subject.key(),
        author: ctx.accounts.author.key(),
        nonce,
        previous_case,
        agreement_hash,
        result_hash,
        mode,
        author_principal,
        challenge_target,
        funding_deadline,
        bump: ctx.bumps.case,
        vault_bump: ctx.bumps.vault,
    }));
    if let Some(deadline) = audit_deadline {
        ctx.accounts.case.start_direct_audit(deadline)?;
    }
    ctx.accounts
        .author_receipt
        .set_inner(StakeReceipt::new_author(
            ctx.accounts.case.key(),
            ctx.accounts.author.key(),
            author_principal,
            ctx.bumps.author_receipt,
        ));
    ctx.accounts.policy.active_uses = active_uses;

    Ok(())
}

/// Validate the protocol-owned per-author attempt chain after Policy signer authorization.
/// Client subject data and lifecycle rules remain the client's responsibility.
fn validate_previous_case(
    subject: &AccountInfo<'_>,
    policy_key: &Pubkey,
    author: &Pubkey,
    nonce: u64,
    remaining: &[AccountInfo<'_>],
) -> Result<Option<Pubkey>> {
    require!(!subject.executable, TekmerionError::InvalidSubject);
    if nonce == 0 {
        require!(remaining.is_empty(), TekmerionError::InvalidHistory);
        return Ok(None);
    }

    require!(remaining.len() == 1, TekmerionError::InvalidHistory);
    let previous_account = &remaining[0];
    let previous_key = previous_account.key();
    require_keys_eq!(
        *previous_account.owner,
        crate::ID,
        TekmerionError::InvalidHistory
    );
    require!(!previous_account.executable, TekmerionError::InvalidHistory);

    let data = previous_account.try_borrow_data()?;
    require!(data.len() == Case::SPACE, TekmerionError::InvalidHistory);
    let previous = Case::try_deserialize(&mut data.as_ref())
        .map_err(|_| error!(TekmerionError::InvalidHistory))?;

    let (expected_key, bump) = Case::address(policy_key, subject.key, author, nonce - 1);
    require!(
        previous_key == expected_key
            && previous.bump == bump
            && previous.policy == *policy_key
            && previous.subject == subject.key()
            && previous.author == *author
            && previous.nonce == nonce - 1,
        TekmerionError::InvalidHistory
    );

    require!(
        previous.phase() == CasePhase::Finalized && previous.finalized_at.is_some(),
        TekmerionError::InvalidHistory
    );
    require!(
        matches!(
            (previous.result(), previous.completion_method()),
            (
                Some(CaseResult::No | CaseResult::Unknown),
                Some(CompletionMethod::Audited)
            ) | (None, Some(CompletionMethod::TechnicalFailure))
        ),
        TekmerionError::InvalidHistory
    );

    Ok(Some(previous_key))
}
