//! Anchor assertion custody, authenticated audit and backed settlement.

use anchor_lang::prelude::*;

pub mod calculations;
mod custody;
pub mod errors;
pub mod instructions;
pub mod network;
pub mod state;

pub use instructions::*;
pub use network::USDC_MINT;
pub use state::*;

declare_id!("TEKxbA5q3EhSDSBeYiW9AT3GGPyzruBSQUcuQXxdy3s");

pub const TOKEN_DECIMALS: u8 = 6;

#[program]
pub mod tekmerion {
    use super::*;

    pub fn create_policy(
        ctx: Context<CreatePolicy>,
        identifier: u64,
        client_authority: Pubkey,
        rules: PolicyRules,
    ) -> Result<()> {
        instructions::create_policy::handle(ctx, identifier, client_authority, rules)
    }

    pub fn update_policy(ctx: Context<UpdatePolicy>, rules: PolicyRules) -> Result<()> {
        instructions::update_policy::handle(ctx, rules)
    }

    pub fn acquire_policy(ctx: Context<AcquirePolicy>) -> Result<()> {
        instructions::acquire_policy::handle(ctx)
    }

    pub fn create_case(
        ctx: Context<CreateCase>,
        nonce: u64,
        funding: CaseFunding,
        agreement_hash: [u8; 32],
        result_hash: [u8; 32],
    ) -> Result<()> {
        instructions::create_case::handle(ctx, nonce, funding, agreement_hash, result_hash)
    }

    pub fn contribute(ctx: Context<Contribute>, requested: u64) -> Result<()> {
        instructions::contribute::handle(ctx, requested)
    }

    pub fn finalize_unchallenged(ctx: Context<FinalizeUnchallenged>) -> Result<()> {
        instructions::finalize_unchallenged::handle(ctx)
    }

    pub fn submit_audit<'info>(
        ctx: Context<'info, SubmitAudit<'info>>,
        answer: AuditAnswer,
        reason: AuditReason,
    ) -> Result<()> {
        instructions::submit_audit::handle(ctx, answer, reason)
    }

    pub fn finalize_timeout(ctx: Context<FinalizeTimeout>) -> Result<()> {
        instructions::finalize_timeout::handle(ctx)
    }

    pub fn claim(ctx: Context<Claim>) -> Result<()> {
        instructions::claim::handle(ctx)
    }
}
