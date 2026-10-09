use anchor_lang::prelude::*;

use crate::{errors::TekmerionError, state::policy::MAX_AUDITORS};

/// Authenticated audit answers. Infrastructure failure is not an answer.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditAnswer {
    Yes,
    No,
    Unknown,
}

impl AuditAnswer {
    pub fn accepts_reason(self, reason: AuditReason) -> bool {
        matches!(
            (self, reason),
            (Self::Yes | Self::No, AuditReason::Decisive)
                | (
                    Self::Unknown,
                    AuditReason::MissingEvidence
                        | AuditReason::StaleEvidence
                        | AuditReason::ConflictingEvidence
                        | AuditReason::UnsupportedEvidence
                        | AuditReason::Inconclusive
                )
        )
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditReason {
    Decisive,
    MissingEvidence,
    StaleEvidence,
    ConflictingEvidence,
    UnsupportedEvidence,
    Inconclusive,
}

/// Retained decision. The terminal response is the vote completing quorum.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditDecision {
    pub answer: AuditAnswer,
    pub reason: AuditReason,
}

impl AuditDecision {
    pub const SPACE: usize = 2;

    pub fn new(answer: AuditAnswer, reason: AuditReason) -> Result<Self> {
        require!(
            answer.accepts_reason(reason),
            TekmerionError::InvalidAuditResponse
        );

        Ok(Self { answer, reason })
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditVote {
    pub auditor_index: u8,
    pub response: AuditDecision,
}

impl AuditVote {
    pub const SPACE: usize = 1 + AuditDecision::SPACE;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditContext {
    pub deadline: Option<i64>,
    // Retains every accepted vote, including other answers and votes before timeout.
    pub votes: Vec<AuditVote>,
    // The vote completing quorum supplies this decision; before quorum it is absent.
    pub response: Option<AuditDecision>,
}

impl AuditContext {
    pub const SPACE: usize = 9 + 4 + MAX_AUDITORS * AuditVote::SPACE + 1 + AuditDecision::SPACE;

    /// Called exactly once when the accepted challenge principal reaches its cap.
    pub(crate) fn start(&mut self, deadline: i64) -> Result<()> {
        require!(
            self.deadline.is_none() && self.response.is_none() && self.votes.is_empty(),
            TekmerionError::InvalidPhase
        );

        self.deadline = Some(deadline);

        Ok(())
    }

    /// Each configured authority gets one immutable vote for the entire Case.
    pub(crate) fn record_vote(&mut self, vote: AuditVote) -> Result<()> {
        require!(
            self.deadline.is_some() && self.response.is_none(),
            TekmerionError::InvalidPhase
        );
        require!(
            self.votes
                .iter()
                .all(|existing| existing.auditor_index != vote.auditor_index),
            TekmerionError::DuplicateAuditVote
        );
        require!(
            usize::from(vote.auditor_index) < MAX_AUDITORS && self.votes.len() < MAX_AUDITORS,
            TekmerionError::InvalidRules
        );

        self.votes.push(vote);

        Ok(())
    }

    /// Stable configuration order makes destination ordering and rounding explicit.
    pub fn matching_auditors(&self, answer: AuditAnswer) -> Vec<u8> {
        let mut indices: Vec<_> = self
            .votes
            .iter()
            .filter(|vote| vote.response.answer == answer)
            .map(|vote| vote.auditor_index)
            .collect();

        indices.sort_unstable();

        indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote(index: u8, answer: AuditAnswer, reason: AuditReason) -> AuditVote {
        AuditVote {
            auditor_index: index,
            response: AuditDecision { answer, reason },
        }
    }

    #[test]
    fn independent_votes_match_on_answer_in_configured_order() {
        let mut audit = AuditContext::default();
        audit.start(100).unwrap();
        audit
            .record_vote(vote(4, AuditAnswer::Unknown, AuditReason::MissingEvidence))
            .unwrap();
        audit
            .record_vote(vote(0, AuditAnswer::No, AuditReason::Decisive))
            .unwrap();
        audit
            .record_vote(vote(
                2,
                AuditAnswer::Unknown,
                AuditReason::ConflictingEvidence,
            ))
            .unwrap();
        assert_eq!(audit.matching_auditors(AuditAnswer::Unknown), vec![2, 4]);
        assert_eq!(audit.matching_auditors(AuditAnswer::No), vec![0]);
        assert!(audit.matching_auditors(AuditAnswer::Yes).is_empty());
    }

    #[test]
    fn changing_or_repeating_a_vote_does_not_add_another_vote() {
        let mut audit = AuditContext::default();
        audit.start(100).unwrap();
        audit
            .record_vote(vote(0, AuditAnswer::Yes, AuditReason::Decisive))
            .unwrap();
        assert!(
            audit
                .record_vote(vote(0, AuditAnswer::No, AuditReason::Decisive))
                .is_err()
        );
        assert!(
            audit
                .record_vote(vote(0, AuditAnswer::Yes, AuditReason::Decisive))
                .is_err()
        );
        assert_eq!(audit.votes.len(), 1);
        assert_eq!(audit.votes[0].response.answer, AuditAnswer::Yes);
    }

    #[test]
    fn maximum_audit_serialization_fits_reserved_space() {
        let mut audit = AuditContext::default();
        audit.start(100).unwrap();
        for index in 0..MAX_AUDITORS {
            audit
                .record_vote(vote(index as u8, AuditAnswer::Yes, AuditReason::Decisive))
                .unwrap();
        }
        audit.response = Some(audit.votes[0].response);
        let mut serialized = Vec::new();
        audit.serialize(&mut serialized).unwrap();
        assert_eq!(serialized.len(), AuditContext::SPACE);
    }
}
