use anchor_lang::prelude::*;

use super::{Case, CasePhase, CaseResult};
use crate::calculations::{
    amounts::UsdcAmount,
    settlement::{self, AuditedOutcome, FundedAllocation, OptimisticRefunds},
};
use crate::{
    errors::{TekmerionError, domain_error},
    state::audit::{AuditAnswer, AuditDecision},
};

impl Case {
    /// Freeze the validated fee-free refund basis after deadline and backing checks.
    pub(crate) fn finalize_optimistic(
        &mut self,
        refunds: OptimisticRefunds,
        now: i64,
    ) -> Result<()> {
        self.require_funding()?;
        require!(
            self.challenge_principal < self.challenge_target
                && refunds.author().base_units() == self.author_principal
                && refunds.contributors().base_units() == self.challenge_principal,
            TekmerionError::InvalidPhase
        );

        self.freeze_rights(
            Some(CaseResult::Yes),
            refunds.author().base_units(),
            u128::from(refunds.contributors().base_units()),
            0,
            now,
        );

        Ok(())
    }

    pub(crate) fn audited_allocation(
        &self,
        answer: AuditAnswer,
        audit_fee: u64,
    ) -> Result<FundedAllocation> {
        self.require_pending_phase(CasePhase::Funded)?;

        settlement::audited_allocation(
            UsdcAmount::from_base_units(self.author_principal),
            UsdcAmount::from_base_units(self.challenge_principal),
            UsdcAmount::from_base_units(audit_fee),
            match answer {
                AuditAnswer::Yes => AuditedOutcome::Yes,
                AuditAnswer::No => AuditedOutcome::No,
                AuditAnswer::Unknown => AuditedOutcome::Unknown,
            },
        )
        .map_err(domain_error)
    }

    pub(crate) fn finalize_audited(
        &mut self,
        decision: AuditDecision,
        allocation: FundedAllocation,
        now: i64,
    ) -> Result<()> {
        self.require_audit_open(now)?;

        let result = match decision.answer {
            AuditAnswer::Yes => CaseResult::Yes,
            AuditAnswer::No => CaseResult::No,
            AuditAnswer::Unknown => CaseResult::Unknown,
        };

        self.audit.response = Some(decision);
        self.freeze_rights(
            Some(result),
            allocation.author().base_units(),
            allocation.contributors().base_units(),
            allocation.audit_fee().base_units(),
            now,
        );

        Ok(())
    }

    pub(crate) fn timeout_allocation(&self, now: i64) -> Result<FundedAllocation> {
        self.require_pending_phase(CasePhase::Funded)?;

        let deadline = self.audit.deadline.ok_or(TekmerionError::InvalidPhase)?;
        require!(self.audit.response.is_none(), TekmerionError::InvalidPhase);
        require!(now >= deadline, TekmerionError::AuditStillOpen);

        settlement::timeout_refunds(
            UsdcAmount::from_base_units(self.author_principal),
            UsdcAmount::from_base_units(self.challenge_principal),
        )
        .map_err(domain_error)
    }

    pub(crate) fn finalize_timeout(
        &mut self,
        allocation: FundedAllocation,
        now: i64,
    ) -> Result<()> {
        self.require_pending_phase(CasePhase::Funded)?;

        self.freeze_rights(
            None,
            allocation.author().base_units(),
            allocation.contributors().base_units(),
            0,
            now,
        );

        Ok(())
    }

    fn freeze_rights(
        &mut self,
        result: Option<CaseResult>,
        author: u64,
        contributors: u128,
        fee: u64,
        now: i64,
    ) {
        self.author_entitlement = author;
        self.contributor_entitlement = contributors;
        self.fee_entitlement = fee;

        self.contributor_unpaid = contributors;

        self.phase = CasePhase::Finalized;
        self.result = result;
        self.finalized_at = Some(now);
    }

    pub(crate) fn require_finalized(&self) -> Result<()> {
        require!(
            self.completion_method().is_some(),
            TekmerionError::InvalidPhase
        );

        Ok(())
    }
}
