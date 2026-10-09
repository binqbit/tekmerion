use crate::support::{Actor, BALANCE, BOND, DURATION, ExpectedError, START, TARGET, TestContext};
use tekmerion::{CasePhase, errors::TekmerionError as Error};
use tekmerion_sdk::SdkError;

#[test]
fn tkt_029_030_last_contributions_cap_actual_credit_in_both_orders() {
    for order in [[Actor::Alice, Actor::Bob], [Actor::Bob, Actor::Alice]] {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 10);
        ctx.contribute(Actor::Bob, 20);
        // Both requests are prepared at C=30; the second executes after C=93.
        let requests = [
            ctx.contribute_ix(order[0], 63),
            ctx.contribute_ix(order[1], 90),
        ];
        ctx.ok(&requests[..1], &order[..1]);
        let last_before = ctx.receipt_state(order[1]).principal;
        ctx.ok(&requests[1..], &order[1..]);
        assert_eq!(ctx.receipt_state(order[1]).principal - last_before, 7);
        assert_eq!(ctx.case_state().unclaimed_contributors, 2);
        assert_eq!(ctx.case_state().challenge_principal, TARGET);
        assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
        assert_eq!(ctx.case_state().result(), None);
        assert_eq!(ctx.case_state().completion_method(), None);
        assert_eq!(ctx.amount(ctx.vault), BOND + TARGET);
        let total =
            ctx.receipt_state(Actor::Alice).principal + ctx.receipt_state(Actor::Bob).principal;
        assert_eq!(total, TARGET);
        for actor in order {
            assert_eq!(
                BALANCE - ctx.amount(ctx.token(actor)),
                ctx.receipt_state(actor).principal
            );
        }
        assert_eq!(ctx.case_state().funding_deadline, START + DURATION as i64);
        ctx.reject(
            &[ctx.contribute_ix(order[0], 1)],
            &[order[0]],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        ctx.set_time(START + DURATION as i64);
        ctx.reject(
            &[ctx.finalize_ix()],
            &[],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
    }
}

#[test]
fn tkt_029_030_positive_contributions_and_topups_accept_one_base_unit() {
    let mut ctx = TestContext::new();
    ctx.create_case();

    assert_eq!(
        ctx.view().quote_contribution(0),
        Err(SdkError::ActionUnavailable)
    );
    assert_eq!(ctx.view().quote_contribution(1), Ok(1));
    ctx.reject(
        &[ctx.contribute_ix(Actor::Alice, 0)],
        &[Actor::Alice],
        ExpectedError::Protocol(Error::InvalidContribution),
    );
    assert!(ctx.get_account(&ctx.receipt(Actor::Alice)).is_none());
    assert_eq!(ctx.case_state().unclaimed_contributors, 0);
    ctx.contribute(Actor::Alice, 1);
    ctx.contribute(Actor::Alice, 1);
    assert_eq!(ctx.receipt_state(Actor::Alice).principal, 2);
    assert_eq!(ctx.case_state().challenge_principal, 2);
    assert_eq!(ctx.case_state().unclaimed_contributors, 1);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE - 2);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Alice, 0)],
        &[Actor::Alice],
        ExpectedError::Protocol(Error::InvalidContribution),
    );

    ctx.contribute(Actor::Alice, 91);
    assert_eq!(ctx.case_state().challenge_principal, 93);
    assert_eq!(ctx.view().quote_contribution(1), Ok(1));
    assert_eq!(ctx.view().quote_contribution(6), Ok(6));
    assert_eq!(ctx.view().quote_contribution(u64::MAX), Ok(7));

    // A positive partial contribution is valid even near the target.
    ctx.contribute(Actor::Bob, 1);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funding);
    assert_eq!(ctx.case_state().challenge_principal, 94);
    assert_eq!(ctx.case_state().unclaimed_contributors, 2);
    assert_eq!(ctx.view().quote_contribution(u64::MAX), Ok(6));
    ctx.contribute(Actor::Bob, u64::MAX);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().challenge_principal, TARGET);
    assert_eq!(ctx.case_state().unclaimed_contributors, 2);
    assert_eq!(ctx.receipt_state(Actor::Alice).principal, 93);
    assert_eq!(ctx.receipt_state(Actor::Bob).principal, 7);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE - 93);
    assert_eq!(ctx.amount(ctx.token(Actor::Bob)), BALANCE - 7);
    assert_eq!(ctx.amount(ctx.vault), BOND + TARGET);
}

#[test]
fn tkt_010_031_032_clock_boundaries_use_canonical_state_and_established_refund_rights() {
    for offset in [-1, 0, 1] {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 25);
        let deadline = ctx.case_state().funding_deadline;
        ctx.set_time(deadline + offset);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        ctx.reject(
            &[ctx.contributor_claim_ix(Actor::Alice)],
            &[Actor::Alice],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        if offset < 0 {
            ctx.reject(
                &[ctx.finalize_ix()],
                &[],
                ExpectedError::Protocol(Error::FundingStillOpen),
            );
            ctx.contribute(Actor::Bob, 75);
            ctx.set_time(deadline);
            ctx.reject(
                &[ctx.finalize_ix()],
                &[],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
        } else {
            ctx.reject(
                &[ctx.contribute_ix(Actor::Bob, 75)],
                &[Actor::Bob],
                ExpectedError::Protocol(Error::FundingClosed),
            );
            ctx.ok(&[ctx.finalize_ix()], &[]);
            ctx.reject(
                &[ctx.contribute_ix(Actor::Alice, 1)],
                &[Actor::Alice],
                ExpectedError::Protocol(Error::InvalidPhase),
            );
            ctx.ok(
                &[
                    ctx.contributor_claim_ix(Actor::Alice),
                    ctx.author_claim_ix(),
                ],
                &[Actor::Author, Actor::Alice],
            );
            assert_eq!(ctx.amount(ctx.vault), 0);
        }
    }
}
