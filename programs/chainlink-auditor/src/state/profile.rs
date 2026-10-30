use anchor_lang::prelude::*;

use crate::{confidential::VERSION, errors::AdapterError};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq, InitSpace)]
pub struct ConfidentialConfig {
    // Delivery authority.
    pub forwarder_program: Pubkey,
    pub forwarder_state: Pubkey,

    // Pinned workflow.
    pub workflow_id: [u8; 32],

    // Commitment to the immutable workflow configuration.
    pub agent_config_hash: [u8; 32],

    // Authenticated technical retries; semantic verdicts are final.
    pub max_attempts: u16,
}

#[account]
#[derive(Debug, InitSpace)]
pub struct ConfidentialProfile {
    // Account identity.
    pub version: u8,
    pub bump: u8,

    // Policy commitment.
    pub policy: Pubkey,
    pub rules_hash: [u8; 32],

    // Immutable auditor configuration.
    pub config: ConfidentialConfig,
}

impl ConfidentialProfile {
    pub const SEED: &'static [u8] = b"chainlink_profile";

    pub fn address(policy: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, policy.as_ref()], &crate::ID)
    }

    pub fn validate_config(&self) -> Result<()> {
        require!(self.version == VERSION, AdapterError::InvalidProfile);
        require!(
            self.config.forwarder_program != Pubkey::default()
                && self.config.forwarder_state != Pubkey::default()
                && self.config.workflow_id != [0; 32]
                && self.config.agent_config_hash != [0; 32]
                && self.config.max_attempts > 0,
            AdapterError::InvalidConfiguration
        );
        Ok(())
    }
}
