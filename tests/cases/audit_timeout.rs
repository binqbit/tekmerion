use crate::support::{Actor, BALANCE, BOND, ExpectedError, TestContext, committee_rules};
use tekmerion::{
    AuditAnswer, CasePhase, CaseResult, CompletionMethod, errors::TekmerionError as Error,
};

#[test]
fn strict_cutoff_requires_a_timely_quorum_and_timeout_refunds_without_fees() {
    for offset in [-1, 0, 1] {
        let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
        ctx.create_case();
        ctx.contribute(Actor::Alice, 40);
        ctx.contribute(Actor::Bob, 60);
        let decision = ctx.audit_decision(AuditAnswer::Unknown);
        ctx.vote(Actor::Auditor1, &decision);
        let deadline = ctx.case_state().audit.deadline.unwrap();
        ctx.set_time(deadline + offset);
        if offset < 0 {
            ctx.reject(
                &[ctx.finalize_timeout_ix()],
                &[],
                ExpectedError::Protocol(Error::AuditStillOpen),
            );
            ctx.vote(Actor::Auditor2, &decision);
            ctx.set_time(deadline + 1);
            ctx.reject(
                &[ctx.finalize_timeout_ix()],
                &[],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
            assert_eq!(ctx.case_state().result(), Some(CaseResult::Unknown));
            assert_eq!(
                ctx.case_state().completion_method(),
                Some(CompletionMethod::Audited)
            );
            assert_eq!(ctx.case_state().author_entitlement, BOND - 7);
            assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
            assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 3);
        } else {
            ctx.reject(
                &[ctx.submit_audit_ix(Actor::Auditor2, &decision)],
                &[Actor::Auditor2],
                ExpectedError::Protocol(Error::AuditExpired),
            );
            ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
            let case = ctx.case_state();
            assert_eq!(case.phase(), CasePhase::Finalized);
            assert_eq!(case.result(), None);
            assert_eq!(
                case.completion_method(),
                Some(CompletionMethod::TechnicalFailure)
            );
            assert_eq!(case.audit.response, None);
            assert_eq!(case.audit.votes.len(), 1);
            assert_eq!(case.author_entitlement, BOND);
            assert_eq!(case.contributor_entitlement, 100);
            assert_eq!(case.fee_entitlement, 0);
            ctx.reject(
                &[ctx.submit_audit_ix(Actor::Auditor2, &decision)],
                &[Actor::Auditor2],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
            ctx.ok(
                &[
                    ctx.author_claim_ix(),
                    ctx.contributor_claim_ix(Actor::Alice),
                    ctx.contributor_claim_ix(Actor::Bob),
                ],
                &[Actor::Author, Actor::Alice, Actor::Bob],
            );
            for actor in [
                Actor::Author,
                Actor::Alice,
                Actor::Bob,
                Actor::FeeRecipient,
                Actor::FeeRecipient2,
            ] {
                assert_eq!(ctx.amount(ctx.token(actor)), BALANCE);
            }
            assert_eq!(ctx.amount(ctx.vault), 0);
        }
    }
}

#[test]
fn conflicting_votes_do_not_form_a_quorum_or_earn_a_fee() {
    let mut ctx = TestContext::with_rules(committee_rules(3, 5), BALANCE);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    for (actor, answer) in [
        (Actor::Auditor1, AuditAnswer::Yes),
        (Actor::Auditor2, AuditAnswer::Yes),
        (Actor::Auditor3, AuditAnswer::No),
        (Actor::Auditor4, AuditAnswer::No),
        (Actor::Auditor5, AuditAnswer::Unknown),
    ] {
        ctx.vote(actor, &ctx.audit_decision(answer));
    }
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 5);
    assert_eq!(ctx.case_state().audit.response, None);
    ctx.set_time(ctx.case_state().audit.deadline.unwrap());
    ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
    assert_eq!(ctx.case_state().fee_entitlement, 0);
    assert_eq!(ctx.amount(ctx.vault), BOND + 100);
}
