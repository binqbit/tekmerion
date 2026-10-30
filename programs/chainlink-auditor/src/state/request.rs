use anchor_lang::prelude::*;
use tekmerion_sdk::program::{AuditDecision, Case};

use super::ConfidentialProfile;
use crate::{
    binding::require_open, confidential::VERSION, errors::AdapterError, payload::decision,
};

mod receive;
pub use receive::ReceiveContext;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum ConfidentialState {
    Prepared,
    Active,
    Retryable,
    Ready,
    Applied,
    Stopped,
}

#[account]
#[derive(Debug, InitSpace)]
pub struct ConfidentialRequest {
    // Canonical identity and the immutable audit task.
    pub version: u8,
    pub bump: u8,
    pub profile: Pubkey,
    pub case: Pubkey,
    pub digest: [u8; 32],

    // One current execution. Historical indexes can never become current again.
    pub state: ConfidentialState,
    pub attempts_started: u16,
    pub execution_id: [u8; 32],
    pub claimed_at: i64,

    // Authenticated terminal message, retained until the next permitted attempt.
    pub terminal_at: i64,
    pub terminal_payload_hash: [u8; 32],
    pub failure_code: u8,

    // A semantic verdict is retained permanently and applied at most once.
    pub report_hash: [u8; 32],
    pub answer: u8,
    pub reason: u8,
    pub applied_at: i64,
}

impl ConfidentialRequest {
    pub const SEED: &'static [u8] = b"chainlink_request";

    pub fn address(profile: &Pubkey, case: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, profile.as_ref(), case.as_ref()], &crate::ID)
    }

    pub fn prepared(profile: Pubkey, case: Pubkey, digest: [u8; 32], bump: u8) -> Self {
        Self {
            version: VERSION,
            bump,
            profile,
            case,
            digest,
            state: ConfidentialState::Prepared,
            attempts_started: 0,
            execution_id: [0; 32],
            claimed_at: 0,
            terminal_at: 0,
            terminal_payload_hash: [0; 32],
            failure_code: 0,
            report_hash: [0; 32],
            answer: 0,
            reason: 0,
            applied_at: 0,
        }
    }

    pub fn can_start(&self, profile: &ConfidentialProfile, case: &Case, now: i64) -> Result<()> {
        require_open(case, now)?;
        require!(
            matches!(
                self.state,
                ConfidentialState::Prepared | ConfidentialState::Retryable
            ),
            AdapterError::InvalidAttempt
        );
        require!(
            self.attempts_started < profile.config.max_attempts,
            AdapterError::AttemptsExhausted
        );
        Ok(())
    }

    pub fn decision(&self, case: &Case, now: i64) -> Result<Option<AuditDecision>> {
        if self.state == ConfidentialState::Applied {
            return Ok(None);
        }
        require!(
            self.state == ConfidentialState::Ready,
            AdapterError::NotReady
        );
        require_open(case, now)?;
        decision(self.answer, self.reason).map(Some)
    }
}
