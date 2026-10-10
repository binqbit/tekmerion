use anchor_lang::prelude::*;

use crate::{
    errors::TekmerionError,
    state::audit::{AuditAnswer, AuditContext},
};

mod claims;
mod funding;
mod settlement;

#[cfg(test)]
mod tests;

/// Lifecycle progress is separate from the assertion's answer.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasePhase {
    Funding,
    Funded,
    Finalized,
}

/// Absence of an answer is represented by Option, never an invented Unknown.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseResult {
    Yes,
    No,
    Unknown,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionMethod {
    Optimistic,
    Audited,
    TechnicalFailure,
}

/// Authenticated creation inputs supplied after the instruction verifies custody.
pub(crate) struct CaseInitialization {
    // Immutable assertion identity and history.
    pub policy: Pubkey,
    pub subject: Pubkey,
    pub author: Pubkey,
    pub nonce: u64,
    pub previous_case: Option<Pubkey>,
    pub agreement_hash: [u8; 32],
    pub result_hash: [u8; 32],

    // Funding terms.
    pub author_principal: u64,
    pub challenge_target: u64,
    pub funding_deadline: i64,

    // PDA metadata.
    pub bump: u8,
    pub vault_bump: u8,
}

#[account]
#[derive(Debug)]
pub struct Case {
    // Immutable assertion identity and history.
    pub policy: Pubkey,
    pub subject: Pubkey,
    pub author: Pubkey,
    pub nonce: u64,
    pub previous_case: Option<Pubkey>,
    pub agreement_hash: [u8; 32],
    pub result_hash: [u8; 32],

    // Funding terms and progress.
    pub author_principal: u64,
    pub challenge_target: u64,
    pub challenge_principal: u64,
    pub funding_deadline: i64,

    // Audit progress.
    pub audit: AuditContext,

    // Lifecycle.
    phase: CasePhase,
    result: Option<CaseResult>,
    /// Chain time of the single terminal transition, unaffected by claims.
    pub finalized_at: Option<i64>,

    // Fixed settlement entitlements.
    pub author_entitlement: u64,
    pub contributor_entitlement: u128,
    pub fee_entitlement: u64,

    // Payment progress by beneficiary role.
    pub contributor_unpaid: u128,
    /// Accepted contributor receipts that have not discharged their claim.
    pub unclaimed_contributors: u64,
    pub fee_claimed: bool,

    // PDA metadata.
    pub bump: u8,
    pub vault_bump: u8,
}

impl Case {
    pub const SPACE: usize = 8
        + 3 * 32
        + 8
        + 33
        + 2 * 32
        + 3 * 8
        + 8
        + AuditContext::SPACE
        + 1
        + 2
        + 9
        + 8
        + 16
        + 8
        + 16
        + 8
        + 1
        + 2;
    pub const SEED: &'static [u8] = b"case";
    pub const VAULT_SEED: &'static [u8] = b"vault";

    pub(crate) fn new(initial: CaseInitialization) -> Self {
        Self {
            policy: initial.policy,
            subject: initial.subject,
            author: initial.author,
            nonce: initial.nonce,
            previous_case: initial.previous_case,
            agreement_hash: initial.agreement_hash,
            result_hash: initial.result_hash,
            author_principal: initial.author_principal,
            challenge_target: initial.challenge_target,
            challenge_principal: 0,
            funding_deadline: initial.funding_deadline,
            audit: AuditContext::default(),
            phase: CasePhase::Funding,
            result: None,
            finalized_at: None,
            author_entitlement: 0,
            contributor_entitlement: 0,
            fee_entitlement: 0,
            contributor_unpaid: 0,
            unclaimed_contributors: 0,
            fee_claimed: false,
            bump: initial.bump,
            vault_bump: initial.vault_bump,
        }
    }

    pub fn phase(&self) -> CasePhase {
        self.phase
    }

    pub fn result(&self) -> Option<CaseResult> {
        self.result
    }

    /// Derive terminal provenance from the retained funding and audit evidence.
    /// Pending phases and inconsistent terminal combinations have no method.
    pub fn completion_method(&self) -> Option<CompletionMethod> {
        if self.phase != CasePhase::Finalized || self.challenge_target == 0 {
            return None;
        }

        let finalized_at = self.finalized_at?;

        match (self.audit.deadline, self.audit.response, self.result) {
            (None, None, Some(CaseResult::Yes)) => {
                if self.challenge_principal >= self.challenge_target
                    || !self.audit.votes.is_empty()
                    || finalized_at < self.funding_deadline
                {
                    return None;
                }

                Some(CompletionMethod::Optimistic)
            }
            (Some(deadline), Some(response), Some(result)) => {
                if self.challenge_principal != self.challenge_target || finalized_at >= deadline {
                    return None;
                }

                let expected_result = match response.answer {
                    AuditAnswer::Yes => CaseResult::Yes,
                    AuditAnswer::No => CaseResult::No,
                    AuditAnswer::Unknown => CaseResult::Unknown,
                };
                if result != expected_result {
                    return None;
                }

                Some(CompletionMethod::Audited)
            }
            (Some(deadline), None, None) => {
                if self.challenge_principal != self.challenge_target || finalized_at < deadline {
                    return None;
                }

                Some(CompletionMethod::TechnicalFailure)
            }
            _ => None,
        }
    }

    pub(crate) fn require_funding(&self) -> Result<()> {
        self.require_pending_phase(CasePhase::Funding)
    }

    pub(crate) fn require_audit_open(&self, now: i64) -> Result<()> {
        self.require_pending_phase(CasePhase::Funded)?;

        let deadline = self.audit.deadline.ok_or(TekmerionError::InvalidPhase)?;
        require!(self.audit.response.is_none(), TekmerionError::InvalidPhase);
        require!(now < deadline, TekmerionError::AuditExpired);

        Ok(())
    }

    fn require_pending_phase(&self, phase: CasePhase) -> Result<()> {
        require!(
            self.phase == phase && self.result.is_none() && self.finalized_at.is_none(),
            TekmerionError::InvalidPhase
        );

        Ok(())
    }

    pub fn address(policy: &Pubkey, subject: &Pubkey, nonce: u64) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[
                Self::SEED,
                policy.as_ref(),
                subject.as_ref(),
                &nonce.to_le_bytes(),
            ],
            &crate::ID,
        )
    }

    pub fn vault_address(case: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::VAULT_SEED, case.as_ref()], &crate::ID)
    }

    pub fn signer_seeds<'a>(&'a self, nonce: &'a [u8; 8], bump: &'a [u8; 1]) -> [&'a [u8]; 5] {
        [
            Self::SEED,
            self.policy.as_ref(),
            self.subject.as_ref(),
            nonce,
            bump,
        ]
    }
}
