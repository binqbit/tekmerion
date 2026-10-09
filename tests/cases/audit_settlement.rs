use crate::support::{Actor, BALANCE, BOND, ExpectedError, TestContext, pk, rules};
use tekmerion::{
    AuditAnswer, CasePhase, CaseResult, CompletionMethod, errors::TekmerionError as Error,
};

#[test]
fn authenticated_answers_conserve_allocations_and_pay_the_last_claimant_remainder() {
    // Literal independent ledger vectors: A=125, T=100, F=7, contributions 20/80.
    for (answer, result, author, contributor_total) in [
        (AuditAnswer::Yes, CaseResult::Yes, 218, 0),
        (AuditAnswer::No, CaseResult::No, 0, 218),
        (AuditAnswer::Unknown, CaseResult::Unknown, 118, 100),
    ] {
        for reverse_claims in [false, true] {
            let (alice, bob) = match answer {
                AuditAnswer::Yes => (0, 0),
                AuditAnswer::No if reverse_claims => (44, 174),
                AuditAnswer::No => (43, 175),
                AuditAnswer::Unknown => (20, 80),
            };
            let mut ctx = TestContext::new();
            ctx.create_case();
            ctx.contribute(Actor::Alice, 20);
            let before_donation = ctx.account(ctx.case);
            let alice_receipt = ctx.account(ctx.receipt(Actor::Alice));
            let mut surplus = 111;
            ctx.ok(
                &[ctx.transfer_ix(
                    ctx.token(Actor::Outsider),
                    ctx.vault,
                    Actor::Outsider,
                    surplus,
                )],
                &[Actor::Outsider],
            );
            assert_eq!(ctx.account(ctx.case), before_donation);
            assert_eq!(ctx.account(ctx.receipt(Actor::Alice)), alice_receipt);
            assert!(ctx.amount(ctx.vault) > BOND + 100);
            let funding = ctx.case_state();
            assert_eq!(funding.phase(), CasePhase::Funding);
            assert_eq!(funding.author_principal, BOND);
            assert_eq!(funding.challenge_principal, 20);
            assert_eq!(funding.unclaimed_contributors, 1);
            assert!(funding.audit.deadline.is_none());
            assert!(funding.audit.response.is_none());
            assert_eq!(ctx.view().quote_contribution(100).unwrap(), 80);
            ctx.contribute(Actor::Bob, 100);
            assert_eq!(ctx.amount(ctx.token(Actor::Bob)), BALANCE - 80);
            assert_eq!(ctx.receipt_state(Actor::Bob).principal, 80);
            assert_eq!(ctx.case_state().challenge_principal, 100);
            assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
            let decision = ctx.audit_decision(answer);
            ctx.deliver(&decision);
            let case = ctx.case_state();
            assert_eq!(case.phase(), CasePhase::Finalized);
            assert_eq!(case.result(), Some(result));
            assert_eq!(case.completion_method(), Some(CompletionMethod::Audited));
            assert_eq!(case.author_entitlement, author);
            assert_eq!(case.contributor_entitlement, contributor_total);
            assert_eq!(case.unclaimed_contributors, 2);
            assert_eq!(case.fee_entitlement, 7);
            assert_eq!(ctx.unpaid(), u128::from(BOND + 100 - 7));
            assert_eq!(ctx.amount(ctx.vault), BOND + 100 - 7 + surplus);
            assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 7);
            if answer == AuditAnswer::No && reverse_claims {
                // Exercise maximum Option occupancy on a host copy only; the runtime
                // journey remains a lawful first attempt with no previous Case.
                let mut maximum_case = ctx.case_state();
                maximum_case.previous_case = Some(pk(ctx.case));
                let mut encoded_case = Vec::new();
                anchor_lang::AccountSerialize::try_serialize(&maximum_case, &mut encoded_case)
                    .unwrap();
                assert!(encoded_case.len() <= tekmerion::Case::SPACE);
                assert_eq!(ctx.account(ctx.case).data.len(), tekmerion::Case::SPACE);
                let mut encoded_policy = Vec::new();
                anchor_lang::AccountSerialize::try_serialize(
                    &ctx.policy_state(),
                    &mut encoded_policy,
                )
                .unwrap();
                assert!(encoded_policy.len() <= tekmerion::Policy::SPACE);
                assert_eq!(ctx.account(ctx.policy).data.len(), tekmerion::Policy::SPACE);
            }
            let immutable = ctx.case_without_claim_progress();
            let subject = ctx.account(ctx.subject);
            ctx.set_time(case.audit.deadline.unwrap() + 1_000_000);
            ctx.reject(
                &[ctx.update_policy_ix(rules())],
                &[Actor::Admin],
                ExpectedError::Protocol(Error::PolicyInUse),
            );
            let order = if reverse_claims {
                [Actor::Bob, Actor::Author, Actor::Alice]
            } else {
                [Actor::Alice, Actor::Author, Actor::Bob]
            };
            let mut unclaimed = 2;
            for actor in order {
                let (claim, paid, principal) = match actor {
                    Actor::Author => (ctx.author_claim_ix(), author, BOND),
                    Actor::Alice => (ctx.contributor_claim_ix(actor), alice, 20),
                    Actor::Bob => (ctx.contributor_claim_ix(actor), bob, 80),
                    _ => unreachable!(),
                };
                ctx.ok(std::slice::from_ref(&claim), &[actor]);
                if matches!(actor, Actor::Alice | Actor::Bob) {
                    unclaimed -= 1;
                    if unclaimed == 1 {
                        let (last_principal, last_payout) = if reverse_claims {
                            (20, alice)
                        } else {
                            (80, bob)
                        };
                        let before_donation = ctx.account(ctx.case);
                        assert_eq!(
                            ctx.case_state().contributor_claim(last_principal).unwrap(),
                            last_payout
                        );
                        ctx.ok(
                            &[ctx.transfer_ix(
                                ctx.token(Actor::Outsider),
                                ctx.vault,
                                Actor::Outsider,
                                13,
                            )],
                            &[Actor::Outsider],
                        );
                        surplus += 13;
                        assert_eq!(ctx.account(ctx.case), before_donation);
                        assert_eq!(
                            ctx.case_state().contributor_claim(last_principal).unwrap(),
                            last_payout,
                            "a later donation cannot increase the final claimant's quote"
                        );
                    }
                }
                assert_eq!(ctx.case_state().unclaimed_contributors, unclaimed);
                assert_eq!(ctx.amount(ctx.token(actor)), BALANCE - principal + paid);
                assert_eq!(
                    u128::from(ctx.amount(ctx.vault)),
                    ctx.unpaid() + u128::from(surplus)
                );
                ctx.reject(
                    &[claim],
                    &[actor],
                    ExpectedError::Protocol(Error::AlreadyClaimed),
                );
                assert_eq!(ctx.case_state().unclaimed_contributors, unclaimed);
                assert_eq!(ctx.case_without_claim_progress(), immutable);
                assert_eq!(ctx.account(ctx.subject), subject);
            }
            assert_eq!(
                ctx.amount(ctx.vault),
                surplus,
                "donation grants nobody a withdrawal right"
            );
            assert_eq!(ctx.unpaid(), 0);
            assert_eq!(ctx.policy_state().active_uses, 1);
            ctx.reject(
                &[ctx.finalize_timeout_ix()],
                &[],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
        }
    }
}

#[test]
fn no_claims_aggregate_topups_and_assign_only_the_remainder_to_the_last_receipt() {
    for vector in 0..5 {
        for reverse_claims in [false, true] {
            let mut ctx = TestContext::new();
            // Literal principal/floor-payout vectors, independently calculated:
            // 33/33/34 with R=118 -> 71/71/74, remainder 2;
            // 50/50 with R=119 -> 109/109, remainder 1;
            // 1/99 with R=118 -> 2/215, remainder 1.
            let (principal, contributions, mut claims, remainder) = match vector {
                0 => (
                    BOND,
                    vec![
                        (Actor::Alice, 13, 1),
                        (Actor::Bob, 17, 2),
                        (Actor::Outsider, 1, 3),
                        (Actor::Alice, 20, 3),
                        (Actor::Bob, 16, 3),
                        (Actor::Outsider, 33, 3),
                    ],
                    vec![
                        (Actor::Alice, 33, 71),
                        (Actor::Bob, 33, 71),
                        (Actor::Outsider, 34, 74),
                    ],
                    2,
                ),
                1 => (
                    126,
                    vec![
                        (Actor::Alice, 1, 1),
                        (Actor::Bob, 49, 2),
                        (Actor::Alice, 49, 2),
                        (Actor::Bob, 1, 2),
                    ],
                    vec![(Actor::Alice, 50, 109), (Actor::Bob, 50, 109)],
                    1,
                ),
                2 => (
                    BOND,
                    vec![(Actor::Alice, 1, 1), (Actor::Bob, 99, 2)],
                    vec![(Actor::Alice, 1, 2), (Actor::Bob, 99, 215)],
                    1,
                ),
                3 => (
                    BOND,
                    vec![(Actor::Alice, 1, 1), (Actor::Alice, 99, 1)],
                    vec![(Actor::Alice, 100, 218)],
                    0,
                ),
                _ => {
                    let mut policy_rules = rules();
                    policy_rules.author_bond = policy_rules.audit_fee;
                    ctx.ok(&[ctx.update_policy_ix(policy_rules)], &[Actor::Admin]);
                    (
                        7,
                        vec![(Actor::Alice, 20, 1), (Actor::Bob, 80, 2)],
                        vec![(Actor::Alice, 20, 20), (Actor::Bob, 80, 80)],
                        0,
                    )
                }
            };
            ctx.ok(&[ctx.submit_case_ix(principal)], &ctx.creation_signers());
            assert_eq!(ctx.author_receipt_state().principal, principal);
            assert_eq!(ctx.author_receipt_state().side, 0);
            assert_eq!(ctx.case_state().unclaimed_contributors, 0);
            for (actor, amount, owners) in contributions {
                ctx.contribute(actor, amount);
                assert_eq!(ctx.case_state().unclaimed_contributors, owners);
            }
            let decision = ctx.audit_decision(AuditAnswer::No);
            ctx.deliver(&decision);
            assert_eq!(ctx.case_state().unclaimed_contributors, claims.len() as u64);
            assert_eq!(
                ctx.case_state().contributor_entitlement,
                u128::from(principal + 100 - 7)
            );
            if reverse_claims {
                claims.reverse();
            }
            let mut remaining = claims.len() as u64;
            for (actor, deposited, floor_payout) in claims {
                let claim = ctx.contributor_claim_ix(actor);
                let payout = floor_payout + if remaining == 1 { remainder } else { 0 };
                if vector == 0 && remaining == 1 {
                    // A failed last claim must preserve both the remainder and its
                    // final-receipt status. Retrying pays the same complete amount.
                    ctx.freeze(ctx.token(actor), true);
                    ctx.reject(
                        std::slice::from_ref(&claim),
                        &[actor],
                        ExpectedError::Protocol(Error::InvalidTokenAccount),
                    );
                    assert_eq!(ctx.case_state().unclaimed_contributors, 1);
                    assert_eq!(ctx.case_state().contributor_unpaid, u128::from(payout));
                    ctx.freeze(ctx.token(actor), false);
                    let late_failure = ctx.transfer_ix(
                        ctx.token(Actor::Outsider),
                        ctx.token(Actor::Bob),
                        Actor::Outsider,
                        BALANCE + 1_000,
                    );
                    ctx.reject_at(
                        &[claim.clone(), late_failure],
                        &[actor, Actor::Outsider],
                        1,
                        ExpectedError::Custom(1),
                    );
                    assert_eq!(ctx.case_state().unclaimed_contributors, 1);
                    assert_eq!(ctx.case_state().contributor_unpaid, u128::from(payout));
                }
                ctx.ok(std::slice::from_ref(&claim), &[actor]);
                remaining -= 1;
                assert_eq!(ctx.case_state().unclaimed_contributors, remaining);
                assert_eq!(ctx.amount(ctx.token(actor)), BALANCE - deposited + payout);
                assert_eq!(u128::from(ctx.amount(ctx.vault)), ctx.unpaid());
                ctx.reject(
                    &[claim],
                    &[actor],
                    ExpectedError::Protocol(Error::AlreadyClaimed),
                );
                assert_eq!(ctx.case_state().unclaimed_contributors, remaining);
            }
            assert_eq!(ctx.case_state().contributor_unpaid, 0);
            assert_eq!(
                ctx.amount(ctx.vault),
                0,
                "the audit fee was already paid at quorum"
            );
            ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
            assert_eq!(ctx.amount(ctx.vault), 0);
        }
    }
}
