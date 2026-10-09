//! Signed auditor decisions and deterministic quorum destinations.
use solana_instruction::Instruction;
use tekmerion::{AuditAnswer, AuditDecision, AuditReason};

use super::{AUDITORS, Actor, FEE_RECIPIENTS, TestContext, pk};

impl TestContext {
    pub fn audit_decision(&self, answer: AuditAnswer) -> AuditDecision {
        AuditDecision {
            answer,
            reason: if answer == AuditAnswer::Unknown {
                AuditReason::Inconclusive
            } else {
                AuditReason::Decisive
            },
        }
    }

    pub fn submit_audit_ix(&self, actor: Actor, decision: &AuditDecision) -> Instruction {
        let policy = self.policy_state();
        let case = self.case_state();
        let signer = pk(self.actor(actor));
        let mut winners: Vec<usize> = case
            .audit
            .votes
            .iter()
            .filter(|vote| vote.response.answer == decision.answer)
            .map(|vote| usize::from(vote.auditor_index))
            .collect();
        if let Some(index) = policy
            .rules
            .auditors
            .iter()
            .position(|auditor| auditor.authority == signer)
            && !winners.contains(&index)
        {
            winners.push(index);
        }
        winners.sort_unstable();
        let destinations = if winners.len() == usize::from(policy.rules.threshold) {
            winners
                .into_iter()
                .map(|index| pk(self.token(FEE_RECIPIENTS[index])))
                .collect()
        } else {
            Vec::new()
        };
        tekmerion_sdk::instructions::submit_audit(
            self.case_address(),
            signer,
            decision.answer,
            decision.reason,
            &destinations,
        )
    }

    pub fn vote(&mut self, actor: Actor, decision: &AuditDecision) {
        self.ok(&[self.submit_audit_ix(actor, decision)], &[actor]);
    }

    /// Complete the default single-auditor policy in one signed transaction.
    pub fn deliver(&mut self, decision: &AuditDecision) {
        self.vote(AUDITORS[0], decision);
        assert_eq!(self.case_state().audit.response, Some(*decision));
        assert!(self.case_state().fee_claimed);
    }
}
