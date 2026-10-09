use crate::support::{Actor, BALANCE, ExpectedError, TARGET, TestContext, rules};
use tekmerion::{
    AuditAnswer, CasePhase, CaseResult, CompletionMethod, errors::TekmerionError as Error,
};
use tekmerion_sdk::accounts::ClaimState;

#[test]
fn every_terminal_result_is_read_directly_and_stays_immutable_through_delayed_claims() {
    for (method, answer, audit_fee) in [
        (CompletionMethod::Optimistic, Some(CaseResult::Yes), 7),
        (CompletionMethod::Audited, Some(CaseResult::Yes), 7),
        (CompletionMethod::Audited, Some(CaseResult::No), 7),
        (CompletionMethod::Audited, Some(CaseResult::Unknown), 7),
        (CompletionMethod::TechnicalFailure, None, 7),
        (CompletionMethod::Audited, Some(CaseResult::Yes), 0),
        (CompletionMethod::Audited, Some(CaseResult::Unknown), 0),
    ] {
        let mut policy_rules = rules();
        policy_rules.audit_fee = audit_fee;
        let mut ctx = TestContext::with_rules(policy_rules, BALANCE);
        let subject = ctx.account(ctx.subject);
        ctx.create_case();
        assert_eq!(ctx.view().case().phase(), CasePhase::Funding);
        assert_eq!(ctx.view().case().result(), None);
        assert_eq!(ctx.view().author_claim(), ClaimState::Unavailable);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        if method == CompletionMethod::Optimistic {
            ctx.contribute(Actor::Alice, 20);
            ctx.finalize_optimistic_at_funding_deadline();
        } else {
            ctx.contribute(Actor::Alice, TARGET);
            assert_eq!(ctx.view().case().phase(), CasePhase::Funded);
            assert_eq!(ctx.view().case().result(), None);
            if method == CompletionMethod::Audited {
                let audit_answer = match answer.unwrap() {
                    CaseResult::Yes => AuditAnswer::Yes,
                    CaseResult::No => AuditAnswer::No,
                    CaseResult::Unknown => AuditAnswer::Unknown,
                };
                ctx.deliver(&ctx.audit_decision(audit_answer));
            } else {
                ctx.set_time(ctx.case_state().audit.deadline.unwrap());
                ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
            }
        }
        let finalized_at = ctx.clock().unix_timestamp;
        let immutable = ctx.case_without_claim_progress();
        let view = ctx.view();
        assert_eq!(view.case().result(), answer);
        assert_eq!(view.case().completion_method(), Some(method));
        assert_eq!(view.case().finalized_at, Some(finalized_at));
        assert!(!view.author_receipt().claimed);
        assert_eq!(view.case().fee_claimed, method == CompletionMethod::Audited);
        assert_eq!(
            view.audit_fees_paid(),
            (method == CompletionMethod::Audited).then_some(audit_fee),
        );
        ctx.set_time(finalized_at + 7 * 86_400);
        ctx.ok(
            &[
                ctx.author_claim_ix(),
                ctx.contributor_claim_ix(Actor::Alice),
            ],
            &[Actor::Author, Actor::Alice],
        );
        assert_eq!(ctx.view().case().result(), answer);
        assert_eq!(ctx.view().case().finalized_at, Some(finalized_at));
        assert_eq!(ctx.view().case().completion_method(), Some(method));
        assert!(ctx.author_receipt_state().claimed);
        assert_eq!(
            ctx.case_state().fee_claimed,
            method == CompletionMethod::Audited
        );
        assert_eq!(ctx.case_without_claim_progress(), immutable);
        assert_eq!(ctx.account(ctx.subject), subject);
        assert_eq!(ctx.unpaid(), 0);
        assert_eq!(ctx.amount(ctx.vault), 0);
        assert_eq!(ctx.policy_state().active_uses, 1);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::AlreadyClaimed),
        );
    }
}
