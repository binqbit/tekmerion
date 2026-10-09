use anchor_lang::prelude::Pubkey;
use tekmerion::{AuditAnswer, AuditReason, CasePhase};

use super::CaseView;
use crate::{Result, SdkError};

/// One fee share if the proposed vote completes quorum in this observation.
/// The recipient is a wallet, not a token account or an assumed associated account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditPayout {
    pub auditor_index: u8,
    pub fee_recipient: Pubkey,
    pub amount: u64,
}

impl CaseView {
    /// Quote a new vote without modifying the observed Case.
    /// Below quorum no payout accounts are needed. At quorum every returned
    /// recipient needs a writable USDC account, including zero-amount shares,
    /// in this order. Concurrent votes can invalidate this quote at execution.
    pub fn quote_audit(
        &self,
        auditor: Pubkey,
        answer: AuditAnswer,
        reason: AuditReason,
    ) -> Result<Vec<AuditPayout>> {
        if self.case.phase() != CasePhase::Funded
            || !answer.accepts_reason(reason)
            || self
                .case
                .audit
                .deadline
                .is_none_or(|deadline| self.observation.unix_timestamp >= deadline)
        {
            return Err(SdkError::ActionUnavailable);
        }
        let rules = &self.policy().rules;
        let auditor_index = rules
            .auditors
            .iter()
            .position(|member| member.authority == auditor)
            .ok_or(SdkError::ActionUnavailable)? as u8;
        if self
            .case
            .audit
            .votes
            .iter()
            .any(|vote| vote.auditor_index == auditor_index)
        {
            return Err(SdkError::ActionUnavailable);
        }

        let mut winners = self.case.audit.matching_auditors(answer);
        winners.push(auditor_index);
        if winners.len() < usize::from(rules.threshold) {
            return Ok(Vec::new());
        }
        winners.sort_unstable();

        let count = winners.len() as u64;
        let base_share = rules.audit_fee / count;
        let remainder = rules.audit_fee % count;
        Ok(winners
            .into_iter()
            .enumerate()
            .map(|(position, auditor_index)| AuditPayout {
                auditor_index,
                fee_recipient: rules.auditors[usize::from(auditor_index)].fee_recipient,
                amount: base_share + u64::from((position as u64) < remainder),
            })
            .collect())
    }
}
