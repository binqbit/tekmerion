use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};

use crate::{Case, TOKEN_DECIMALS, USDC_MINT, errors::TekmerionError};

fn require_valid_token_account(account: &Account<'_, TokenAccount>) -> Result<()> {
    require_keys_eq!(account.mint, USDC_MINT, TekmerionError::InvalidTokenAccount);
    require!(
        account.state == token::spl_token::state::AccountState::Initialized
            && account.is_native.is_none(),
        TekmerionError::InvalidTokenAccount
    );

    Ok(())
}

pub(crate) fn require_valid_vault(
    account: &Account<'_, TokenAccount>,
    authority: &Pubkey,
) -> Result<()> {
    require_valid_token_account(account)?;
    require_keys_eq!(
        account.owner,
        *authority,
        TekmerionError::InvalidTokenAccount
    );
    require!(
        account.delegate.is_none() && account.close_authority.is_none(),
        TekmerionError::InvalidTokenAccount
    );

    Ok(())
}

pub(crate) fn require_backing(balance: u64, liability: u128) -> Result<()> {
    require!(
        u128::from(balance) >= liability,
        TekmerionError::InsufficientBacking
    );

    Ok(())
}

/// Transfers only after account checks, then authenticates actual debit and credit.
pub(crate) fn transfer<'info>(
    from: &mut Account<'info, TokenAccount>,
    to: &mut Account<'info, TokenAccount>,
    mint: &Account<'info, Mint>,
    authority: AccountInfo<'info>,
    token_program: &Program<'info, Token>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    require_keys_neq!(from.key(), to.key(), TekmerionError::InvalidTokenAccount);
    require_valid_token_account(from)?;
    require_valid_token_account(to)?;
    require!(
        mint.decimals == TOKEN_DECIMALS,
        TekmerionError::InvalidTokenAccount
    );

    let before_from = from.amount;
    let before_to = to.amount;

    token::transfer_checked(
        CpiContext::new_with_signer(
            token_program.key(),
            TransferChecked {
                from: from.to_account_info(),
                mint: mint.to_account_info(),
                to: to.to_account_info(),
                authority,
            },
            signer_seeds,
        ),
        amount,
        TOKEN_DECIMALS,
    )?;

    from.reload()?;
    to.reload()?;
    require!(
        before_from.checked_sub(from.amount) == Some(amount)
            && to.amount.checked_sub(before_to) == Some(amount),
        TekmerionError::InvalidTransfer
    );

    Ok(())
}

pub(crate) fn pay<'info>(
    case: &Account<'info, Case>,
    source: &mut Account<'info, TokenAccount>,
    destination: &mut Account<'info, TokenAccount>,
    mint: &Account<'info, Mint>,
    token_program: &Program<'info, Token>,
    amount: u64,
    outstanding_liability: u128,
) -> Result<()> {
    require_valid_vault(source, &case.key())?;
    require_backing(source.amount, outstanding_liability)?;
    require_valid_token_account(destination)?;

    // A zero payout still requires valid custody and backing before discharge.
    if amount == 0 {
        return Ok(());
    }

    let nonce = case.nonce.to_le_bytes();
    let bump = [case.bump];
    let seeds = case.signer_seeds(&nonce, &bump);

    transfer(
        source,
        destination,
        mint,
        case.to_account_info(),
        token_program,
        amount,
        &[&seeds],
    )
}
