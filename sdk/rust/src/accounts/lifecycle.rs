use tekmerion::calculations::{
    amounts::UsdcAmount,
    settlement::{self, AuditedOutcome},
    time::{ChainDeadline, ChainTimestamp},
};
use tekmerion::{
    AuditAnswer, Case, CaseMode, CasePhase, CaseResult, CompletionMethod, MAX_AUDITORS, Policy,
};

use crate::{Result, SdkError};

/// Validate the supported lifecycle before presenting any rights.
/// This checks consistency; it cannot prove the supplied bytes came from a chain.
pub(super) fn validate(case: &Case, policy: &Policy, author_claimed: bool) -> Result<u128> {
    let rules = &policy.rules;
    if !rules.has_valid_parameters()
        || case.audit.votes.len() > rules.auditors.len()
        || case.agreement_hash == [0; 32]
        || case.result_hash == [0; 32]
    {
        return Err(SdkError::InvalidState);
    }
    let mut seen = [false; MAX_AUDITORS];
    let mut votes_by_answer = [0_u8; 3];
    for vote in &case.audit.votes {
        let index = usize::from(vote.auditor_index);
        if index >= rules.auditors.len()
            || seen[index]
            || !vote.response.answer.accepts_reason(vote.response.reason)
        {
            return Err(SdkError::InvalidState);
        }
        seen[index] = true;
        votes_by_answer[answer_index(vote.response.answer)] += 1;
    }
    match case.audit.response {
        Some(response)
            if case.phase() == CasePhase::Finalized
                && case.completion_method() == Some(CompletionMethod::Audited)
                && case.audit.votes.last().map(|vote| vote.response) == Some(response)
                && votes_by_answer[answer_index(response.answer)] == rules.threshold => {}
        None if votes_by_answer.iter().all(|count| *count < rules.threshold) => {}
        _ => return Err(SdkError::InvalidState),
    }
    let previous_case = case
        .nonce
        .checked_sub(1)
        .map(|nonce| Case::address(&case.policy, &case.subject, &case.author, nonce).0);
    if case.previous_case != previous_case
        || policy.active_uses == 0
        || match case.mode {
            CaseMode::Challenge => {
                case.author_principal < rules.author_bond
                    || case.challenge_target != rules.challenge_target
            }
            CaseMode::DirectAudit => {
                case.author_principal != rules.audit_fee
                    || case.challenge_target != 0
                    || case.challenge_principal != 0
                    || case.audit.deadline
                        != ChainDeadline::checked_after(
                            ChainTimestamp::from_unix_seconds(case.funding_deadline),
                            rules.audit_timeout,
                        )
                        .ok()
                        .map(ChainDeadline::unix_seconds)
            }
        }
        || case.challenge_principal > case.challenge_target
        || case
            .author_principal
            .checked_add(case.challenge_target)
            .is_none()
        || case.unclaimed_contributors > case.challenge_principal
    {
        return Err(SdkError::InvalidState);
    }
    let funded = case.challenge_principal == case.challenge_target;
    let total = u128::from(case.author_principal) + u128::from(case.challenge_principal);
    match case.phase() {
        CasePhase::Funding | CasePhase::Funded => {
            let valid_phase = match case.phase() {
                CasePhase::Funding => {
                    case.mode == CaseMode::Challenge
                        && !funded
                        && case.audit.deadline.is_none()
                        && case.audit.response.is_none()
                        && case.audit.votes.is_empty()
                }
                CasePhase::Funded => {
                    funded && case.audit.deadline.is_some() && case.audit.response.is_none()
                }
                _ => false,
            };
            if !valid_phase
                || case.result().is_some()
                || case.finalized_at.is_some()
                || case.author_entitlement != 0
                || case.contributor_entitlement != 0
                || case.fee_entitlement != 0
                || case.contributor_unpaid != 0
                || author_claimed
                || case.fee_claimed
                || (case.challenge_principal == 0) != (case.unclaimed_contributors == 0)
            {
                return Err(SdkError::InvalidState);
            }
            Ok(total)
        }
        CasePhase::Finalized => {
            if case.finalized_at.is_none() {
                return Err(SdkError::InvalidState);
            }
            let expected = match (case.completion_method(), case.result()) {
                (Some(CompletionMethod::Optimistic), Some(CaseResult::Yes))
                    if !funded
                        && case.audit.deadline.is_none()
                        && case.audit.response.is_none()
                        && case.audit.votes.is_empty()
                        && !case.fee_claimed =>
                {
                    (
                        case.author_principal,
                        u128::from(case.challenge_principal),
                        0,
                    )
                }
                (Some(CompletionMethod::TechnicalFailure), None)
                    if funded
                        && case.audit.deadline.is_some()
                        && case.audit.response.is_none()
                        && !case.fee_claimed =>
                {
                    (
                        case.author_principal,
                        u128::from(case.challenge_principal),
                        0,
                    )
                }
                (Some(CompletionMethod::Audited), Some(result))
                    if funded && case.audit.deadline.is_some() && case.fee_claimed =>
                {
                    let response = case.audit.response.ok_or(SdkError::InvalidState)?;
                    let outcome = match (response.answer, result) {
                        (AuditAnswer::Yes, CaseResult::Yes) => AuditedOutcome::Yes,
                        (AuditAnswer::No, CaseResult::No) => AuditedOutcome::No,
                        (AuditAnswer::Unknown, CaseResult::Unknown) => AuditedOutcome::Unknown,
                        _ => return Err(SdkError::InvalidState),
                    };
                    let allocation = if case.mode == CaseMode::DirectAudit {
                        settlement::direct_audit_allocation(UsdcAmount::from_base_units(
                            rules.audit_fee,
                        ))
                    } else {
                        settlement::audited_allocation(
                            UsdcAmount::from_base_units(case.author_principal),
                            UsdcAmount::from_base_units(case.challenge_target),
                            UsdcAmount::from_base_units(rules.audit_fee),
                            outcome,
                        )
                        .map_err(|_| SdkError::Arithmetic)?
                    };
                    (
                        allocation.author().base_units(),
                        allocation.contributors().base_units(),
                        allocation.audit_fee().base_units(),
                    )
                }
                _ => return Err(SdkError::InvalidState),
            };
            if (
                case.author_entitlement,
                case.contributor_entitlement,
                case.fee_entitlement,
            ) != expected
                || case.contributor_unpaid > case.contributor_entitlement
                || (case.unclaimed_contributors == 0 && case.contributor_unpaid != 0)
                || (case.contributor_entitlement != 0
                    && case.contributor_unpaid < u128::from(case.unclaimed_contributors))
            {
                return Err(SdkError::InvalidState);
            }
            // Validated entitlements fit actual custody, so this addition cannot overflow.
            Ok(case.outstanding_liability(author_claimed))
        }
    }
}

fn answer_index(answer: AuditAnswer) -> usize {
    match answer {
        AuditAnswer::Yes => 0,
        AuditAnswer::No => 1,
        AuditAnswer::Unknown => 2,
    }
}
