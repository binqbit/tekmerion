use crate::support::{Actor, BALANCE, BOND, ExpectedError, TestContext, rules};
use tekmerion::{CasePhase, CaseResult, CompletionMethod, errors::TekmerionError as Error};

#[test]
fn tkt_001_002_053_057_fee_free_journey_preserves_unpaid_rights_and_policy_lock() {
    for contributions in [false, true] {
        for author_first in [false, true] {
            let mut ctx = TestContext::new();
            let mut edited = rules();
            edited.audit_fee = 9;
            ctx.ok(&[ctx.update_policy_ix(edited.clone())], &[Actor::Admin]);
            let subject_before = ctx.account(ctx.subject);
            let meta = ctx.ok(&[ctx.submit_case_ix(BOND)], &ctx.creation_signers());
            assert!(meta.logs.iter().any(|line| {
                line.contains(&format!("Program {} invoke [1]", crate::support::MAIN))
            }));
            assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
            assert_eq!(ctx.amount(ctx.vault), BOND);
            assert_eq!(ctx.account(ctx.subject), subject_before);
            assert!(ctx.case_state().previous_case.is_none());
            assert_eq!(ctx.case_state().assertion, [0x91; 32]);
            assert_eq!(ctx.case_state().phase(), CasePhase::Funding);
            assert_eq!(ctx.case_state().result(), None);
            assert_eq!(ctx.case_state().completion_method(), None);
            assert_eq!(ctx.policy_state().active_uses, 1);
            if contributions {
                ctx.contribute(Actor::Alice, 13);
                ctx.contribute(Actor::Bob, 17);
                ctx.contribute(Actor::Alice, 7);
                assert_eq!(ctx.receipt_state(Actor::Alice).principal, 20);
                assert_eq!(ctx.amount(ctx.vault), BOND + 37);
                assert_eq!(ctx.case_state().result(), None);
                assert_eq!(ctx.case_state().completion_method(), None);
            }
            ctx.ok(
                &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 11)],
                &[Actor::Outsider],
            );
            assert_eq!(
                ctx.case_state().challenge_principal,
                if contributions { 37 } else { 0 }
            );
            ctx.reject(
                &[ctx.update_policy_ix(edited.clone())],
                &[Actor::Admin],
                ExpectedError::Protocol(Error::PolicyInUse),
            );
            ctx.finalize_optimistic_at_funding_deadline();
            let terminal = ctx.case_state();
            assert_eq!(terminal.phase(), CasePhase::Finalized);
            assert_eq!(terminal.result(), Some(CaseResult::Yes));
            assert_eq!(
                terminal.completion_method(),
                Some(CompletionMethod::Optimistic)
            );
            assert_eq!(terminal.author_entitlement, BOND);
            assert_eq!(
                terminal.contributor_entitlement,
                if contributions { 37 } else { 0 }
            );
            let immutable = ctx.case_without_claim_progress();
            let subject_bytes = ctx.account(ctx.subject).data;
            if author_first {
                ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
            }
            if contributions {
                for actor in [Actor::Bob, Actor::Alice] {
                    ctx.ok(&[ctx.contributor_claim_ix(actor)], &[actor]);
                    assert_eq!(ctx.amount(ctx.token(actor)), BALANCE);
                    assert!(ctx.receipt_state(actor).claimed);
                    let case = ctx.case_state();
                    assert_eq!(
                        u128::from(ctx.amount(ctx.vault)),
                        case.contributor_unpaid
                            + if ctx.author_receipt_state().claimed {
                                0
                            } else {
                                u128::from(BOND)
                            }
                            + 11
                    );
                    ctx.reject(
                        &[ctx.contributor_claim_ix(actor)],
                        &[actor],
                        ExpectedError::Protocol(Error::AlreadyClaimed),
                    );
                }
            }
            if !author_first {
                ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
            }
            assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
            assert_eq!(
                ctx.amount(ctx.vault),
                11,
                "unallocated donation is not an entitlement or fee"
            );
            assert_eq!(ctx.case_state().contributor_unpaid, 0);
            assert_eq!(ctx.amount(ctx.token(Actor::Admin)), BALANCE);
            assert_eq!(ctx.case_without_claim_progress(), immutable);
            assert_eq!(ctx.account(ctx.subject).data, subject_bytes);
            assert_eq!(
                ctx.policy_state().active_uses,
                1,
                "claims never prove client dependency completion"
            );
            ctx.reject(
                &[ctx.author_claim_ix()],
                &[Actor::Author],
                ExpectedError::Protocol(Error::AlreadyClaimed),
            );
            ctx.reject(
                &[ctx.update_policy_ix(edited)],
                &[Actor::Admin],
                ExpectedError::Protocol(Error::PolicyInUse),
            );
            ctx.reject(
                &[ctx.finalize_ix()],
                &[],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
        }
    }
}
