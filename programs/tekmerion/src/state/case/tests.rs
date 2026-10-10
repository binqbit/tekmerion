use super::*;
use crate::{AuditDecision, AuditReason, AuditVote, MAX_AUDITORS};

fn fresh_case() -> Case {
    Case::new(CaseInitialization {
        policy: Pubkey::new_unique(),
        subject: Pubkey::new_unique(),
        author: Pubkey::new_unique(),
        nonce: 0,
        previous_case: None,
        agreement_hash: [0xa1; 32],
        result_hash: [1; 32],
        author_principal: 125,
        challenge_target: 100,
        funding_deadline: 100,
        bump: 255,
        vault_bump: 254,
    })
}

fn response() -> AuditDecision {
    AuditDecision {
        answer: AuditAnswer::Yes,
        reason: AuditReason::Decisive,
    }
}

fn optimistic_case() -> Case {
    let mut case = fresh_case();
    case.challenge_principal = 40;
    case.phase = CasePhase::Finalized;
    case.result = Some(CaseResult::Yes);
    case.finalized_at = Some(100);
    case
}

fn audited_case() -> Case {
    let mut case = fresh_case();
    case.challenge_principal = case.challenge_target;
    case.audit.deadline = Some(200);
    case.audit.votes.push(AuditVote {
        auditor_index: 0,
        response: response(),
    });
    case.audit.response = Some(response());
    case.phase = CasePhase::Finalized;
    case.result = Some(CaseResult::Yes);
    case.finalized_at = Some(199);
    case
}

fn timeout_case() -> Case {
    let mut case = audited_case();
    case.audit.response = None;
    case.result = None;
    case.finalized_at = Some(200);
    case
}

fn rejects_mutations(make_case: fn() -> Case, mutations: &[fn(&mut Case)]) {
    for (index, mutate) in mutations.iter().enumerate() {
        let mut case = make_case();
        mutate(&mut case);
        assert_eq!(case.completion_method(), None, "mutation {index}");
        assert!(case.require_finalized().is_err(), "mutation {index}");
    }
}

#[test]
fn audited_method_rejects_nonterminal_or_inconsistent_evidence() {
    assert_eq!(
        audited_case().completion_method(),
        Some(CompletionMethod::Audited)
    );
    rejects_mutations(
        audited_case,
        &[
            |case| case.phase = CasePhase::Funding,
            |case| case.phase = CasePhase::Funded,
            |case| case.finalized_at = None,
            |case| case.result = None,
            |case| case.result = Some(CaseResult::No),
            |case| case.challenge_principal -= 1,
            |case| case.challenge_target = 0,
            |case| case.audit.deadline = None,
            |case| case.audit.response = None,
            |case| case.finalized_at = case.audit.deadline,
        ],
    );
}

#[test]
fn optimistic_method_rejects_audit_state_or_invalid_funding() {
    assert_eq!(
        optimistic_case().completion_method(),
        Some(CompletionMethod::Optimistic)
    );
    rejects_mutations(
        optimistic_case,
        &[
            |case| case.audit.response = Some(response()),
            |case| case.audit.deadline = Some(200),
            |case| {
                case.audit.votes.push(AuditVote {
                    auditor_index: 0,
                    response: response(),
                });
            },
            |case| case.result = Some(CaseResult::No),
            |case| case.result = Some(CaseResult::Unknown),
            |case| case.result = None,
            |case| case.challenge_principal = case.challenge_target,
            |case| case.challenge_principal = case.challenge_target + 1,
            |case| case.finalized_at = Some(case.funding_deadline - 1),
        ],
    );
}

#[test]
fn timeout_method_allows_partial_votes_but_rejects_a_result_or_invalid_deadline() {
    let case = timeout_case();
    assert_eq!(case.audit.votes.len(), 1);
    assert_eq!(
        case.completion_method(),
        Some(CompletionMethod::TechnicalFailure)
    );
    rejects_mutations(
        timeout_case,
        &[
            |case| case.result = Some(CaseResult::Yes),
            |case| case.result = Some(CaseResult::Unknown),
            |case| case.audit.response = Some(response()),
            |case| case.audit.deadline = None,
            |case| case.challenge_principal -= 1,
            |case| case.finalized_at = Some(case.audit.deadline.unwrap() - 1),
        ],
    );
}

#[test]
fn maximum_case_serialization_fits_reserved_space_without_stored_method() {
    let mut case = audited_case();
    case.previous_case = Some(Pubkey::new_unique());
    case.nonce = 1;
    for index in 1..MAX_AUDITORS {
        case.audit.votes.push(AuditVote {
            auditor_index: index as u8,
            response: response(),
        });
    }
    let mut serialized = Vec::new();
    case.try_serialize(&mut serialized).unwrap();
    assert_eq!(serialized.len(), Case::SPACE);
}
