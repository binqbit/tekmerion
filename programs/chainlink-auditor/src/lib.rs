//! Authenticated Chainlink reports become ordinary Tekmerion PDA votes.
//! No operator instruction can choose an answer or bypass the core deadline.

use anchor_lang::prelude::*;

pub mod binding;
pub mod confidential;
pub mod errors;
pub mod instructions;
pub mod payload;
pub mod report;
pub mod state;

pub use instructions::*;
pub use state::{
    ConfidentialConfig, ConfidentialProfile, ConfidentialRequest, ConfidentialState, ReceiveContext,
};

#[cfg(any(
    all(feature = "mainnet", feature = "devnet"),
    not(any(feature = "mainnet", feature = "devnet"))
))]
compile_error!("Select exactly one network: mainnet or devnet");

declare_id!("CLAxRc3x7pW8DZFs8ypRVGVvEJjHeMxvTwn4HcyC7rtm");

#[program]
pub mod chainlink_auditor {
    use super::*;

    pub fn on_report<'info>(
        ctx: Context<'info, OnReport<'info>>,
        metadata: Vec<u8>,
        report: Vec<u8>,
    ) -> Result<()> {
        instructions::receive::handle(ctx, &metadata, &report)
    }

    pub fn register_confidential_profile(
        ctx: Context<RegisterConfidentialProfile>,
        config: state::ConfidentialConfig,
    ) -> Result<()> {
        instructions::register_profile::handle(ctx, config)
    }

    pub fn prepare_confidential_request(ctx: Context<PrepareConfidentialRequest>) -> Result<()> {
        instructions::prepare_request::handle(ctx)
    }

    pub fn apply_confidential_verdict<'info>(
        ctx: Context<'info, ApplyConfidentialVerdict<'info>>,
    ) -> Result<()> {
        instructions::apply::handle(ctx)
    }
}
