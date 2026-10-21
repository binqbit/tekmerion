use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, TestContext, account_meta_mut, committee_rules, pk,
};
use tekmerion::{
    AuditAnswer, CaseFunding, CaseMode, CasePhase, CompletionMethod,
    errors::TekmerionError as Error,
};
use tekmerion_sdk::{
    accounts::ClaimState,
    instructions::{self as sdk, CreateCase},
};

fn direct(ctx: &mut TestContext) {
    ctx.ok(
        &[ctx.direct_case_ix(ctx.policy_state().rules.audit_fee)],
        &ctx.creation_signers(),
    );
}

#[test]
fn direct_answers_consume_only_the_reviewed_fee_and_never_reward_the_author() {
    for answer in [AuditAnswer::Yes, AuditAnswer::No, AuditAnswer::Unknown] {
        let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
        let now = ctx.clock().unix_timestamp;
        direct(&mut ctx);
        let case = ctx.case_state();
        assert_eq!(case.mode, CaseMode::DirectAudit);
        assert_eq!(case.phase(), CasePhase::Funded);
        assert_eq!(case.author_principal, 7);
        assert_eq!(
            (
                case.challenge_target,
                case.challenge_principal,
                case.unclaimed_contributors
            ),
            (0, 0, 0)
        );
        assert_eq!(case.funding_deadline, now);
        assert_eq!(
            case.audit.deadline,
            Some(now + i64::try_from(ctx.policy_state().rules.audit_timeout).unwrap())
        );
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 7);
        assert_eq!(ctx.amount(ctx.vault), 7);
        assert_eq!(ctx.author_receipt_state().principal, 7);
        assert_eq!(ctx.view().author_claim(), ClaimState::Unavailable);
        let decision = ctx.audit_decision(answer);
        ctx.vote(Actor::Auditor3, &decision);
        assert_eq!(ctx.amount(ctx.vault), 7, "one vote earns no fee");
        ctx.vote(Actor::Auditor1, &decision);
        assert_eq!(
            ctx.case_state().completion_method(),
            Some(CompletionMethod::Audited)
        );
        assert_eq!(ctx.case_state().author_entitlement, 0);
        assert_eq!(ctx.case_state().contributor_entitlement, 0);
        assert_eq!(ctx.case_state().fee_entitlement, 7);
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient3)), BALANCE + 3);
        assert_eq!(ctx.amount(ctx.vault), 0);
        assert_eq!(
            ctx.view().author_claim(),
            ClaimState::Unclaimed { amount: 0 }
        );
        ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::AlreadyClaimed),
        );
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 7);
    }
}

#[test]
fn direct_audit_cannot_be_challenged_or_accepted_optimistically() {
    let mut ctx = TestContext::new();
    direct(&mut ctx);
    for actor in [Actor::Author, Actor::Alice] {
        ctx.reject(
            &[ctx.contribute_ix(actor, 100)],
            &[actor],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
    }
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
    ctx.set_time(ctx.case_state().audit.deadline.unwrap());
    ctx.reject(
        &[ctx.finalize_ix()],
        &[],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
}

#[test]
fn direct_timeout_refunds_author_exactly_once_and_ignores_unsolicited_tokens() {
    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    direct(&mut ctx);
    let decision = ctx.audit_decision(AuditAnswer::No);
    ctx.vote(Actor::Auditor1, &decision);
    ctx.ok(
        &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 13)],
        &[Actor::Outsider],
    );
    let deadline = ctx.case_state().audit.deadline.unwrap();
    ctx.set_time(deadline - 1);
    ctx.reject(
        &[ctx.finalize_timeout_ix()],
        &[],
        ExpectedError::Protocol(Error::AuditStillOpen),
    );
    ctx.set_time(deadline);
    ctx.reject(
        &[ctx.submit_audit_ix(Actor::Auditor2, &decision)],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::AuditExpired),
    );
    ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
    assert_eq!(
        ctx.case_state().completion_method(),
        Some(CompletionMethod::TechnicalFailure)
    );
    assert_eq!(ctx.case_state().result(), None);
    assert_eq!(ctx.case_state().fee_entitlement, 0);
    assert_eq!(
        ctx.view().author_claim(),
        ClaimState::Unclaimed { amount: 7 }
    );
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    ctx.reject(
        &[ctx.author_claim_ix()],
        &[Actor::Author],
        ExpectedError::Protocol(Error::AlreadyClaimed),
    );
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    assert_eq!(ctx.amount(ctx.vault), 13);
    assert_eq!(ctx.unpaid(), 0);
}

#[test]
fn direct_creation_binds_fee_and_author_token_ownership_atomically() {
    let mut ctx = TestContext::new();
    for fee in [0, 6, 8, u64::MAX] {
        ctx.reject(
            &[ctx.direct_case_ix(fee)],
            &ctx.creation_signers(),
            ExpectedError::Protocol(Error::InvalidAuditFee),
        );
        assert!(ctx.get_account(&ctx.case).is_none());
        assert_eq!(ctx.policy_state().active_uses, 0);
    }
    let mut ix = ctx.direct_case_ix(7);
    account_meta_mut(&mut ix, ctx.token(Actor::Author)).pubkey = ctx.token(Actor::Alice);
    ctx.reject(
        &[ix],
        &ctx.creation_signers(),
        ExpectedError::Anchor(anchor_lang::error::ErrorCode::ConstraintTokenOwner),
    );
    direct(&mut ctx);
    assert_eq!(ctx.policy_state().active_uses, 1);
}

#[test]
fn direct_fee_transfer_failure_rolls_back_quorum_and_allows_the_same_vote_to_retry() {
    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    direct(&mut ctx);
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    ctx.vote(Actor::Auditor1, &decision);
    ctx.freeze(ctx.token(Actor::FeeRecipient2), true);
    ctx.reject(
        &[ctx.submit_audit_ix(Actor::Auditor2, &decision)],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    assert_eq!(ctx.amount(ctx.vault), 7);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    ctx.freeze(ctx.token(Actor::FeeRecipient2), false);
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().audit.votes.len(), 2);
    assert_eq!(ctx.amount(ctx.vault), 0);
}

#[test]
fn zero_fee_direct_audits_keep_valid_receipts_for_answers_and_timeout() {
    for answer in [
        Some(AuditAnswer::Yes),
        Some(AuditAnswer::No),
        Some(AuditAnswer::Unknown),
        None,
    ] {
        let mut rules = committee_rules(1, 1);
        rules.audit_fee = 0;
        let mut ctx = TestContext::with_rules(rules, BALANCE);
        direct(&mut ctx);
        assert_eq!(ctx.author_receipt_state().principal, 0);
        assert_eq!(ctx.view().vault_balance(), 0);
        if let Some(answer) = answer {
            ctx.deliver(&ctx.audit_decision(answer));
        } else {
            ctx.set_time(ctx.case_state().audit.deadline.unwrap());
            ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
        }
        assert_eq!(
            ctx.view().author_claim(),
            ClaimState::Unclaimed { amount: 0 }
        );
        ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
        assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::AlreadyClaimed),
        );
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    }
}

#[test]
fn direct_attempts_obey_the_same_author_predecessor_rules() {
    let mut ctx = TestContext::new();
    direct(&mut ctx);
    let address = sdk::CaseAddress {
        nonce: 1,
        ..ctx.case_address()
    };
    let retry = sdk::create_case(
        address,
        CreateCase {
            payer: pk(ctx.actor(Actor::Payer)),
            client_authority: pk(ctx.authority),
            author: pk(ctx.actor(Actor::Author)),
            source: pk(ctx.token(Actor::Author)),
            funding: CaseFunding::DirectAudit { expected_fee: 7 },
            agreement_hash: [0xa1; 32],
            result_hash: [0x92; 32],
        },
    );
    ctx.reject(
        std::slice::from_ref(&retry),
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    ctx.deliver(&ctx.audit_decision(AuditAnswer::No));
    ctx.ok(&[retry], &ctx.creation_signers());
    assert!(
        ctx.get_account(&solana_address::Address::new_from_array(
            address.address().to_bytes()
        ))
        .is_some()
    );
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 14);
}

#[test]
fn a_fresh_retry_can_change_mode_without_reopening_or_spending_the_previous_case() {
    for previous_direct in [false, true] {
        for answer in [
            Some(AuditAnswer::No),
            Some(AuditAnswer::Unknown),
            None,
            Some(AuditAnswer::Yes),
        ] {
            let mut ctx = TestContext::new();
            if previous_direct {
                direct(&mut ctx);
            } else {
                ctx.create_case();
                ctx.contribute(Actor::Alice, 100);
            }
            let next = sdk::CaseAddress {
                nonce: 1,
                ..ctx.case_address()
            };
            let funding = if previous_direct {
                CaseFunding::Bonded { principal: BOND }
            } else {
                CaseFunding::DirectAudit { expected_fee: 7 }
            };
            let retry = sdk::create_case(
                next,
                CreateCase {
                    payer: pk(ctx.actor(Actor::Payer)),
                    client_authority: pk(ctx.authority),
                    author: pk(ctx.actor(Actor::Author)),
                    source: pk(ctx.token(Actor::Author)),
                    funding,
                    agreement_hash: [0xa1; 32],
                    result_hash: [0x92; 32],
                },
            );
            ctx.reject(
                std::slice::from_ref(&retry),
                &ctx.creation_signers(),
                ExpectedError::Protocol(Error::InvalidHistory),
            );
            if let Some(answer) = answer {
                ctx.deliver(&ctx.audit_decision(answer));
            } else {
                ctx.set_time(ctx.case_state().audit.deadline.unwrap());
                ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
            }
            if answer == Some(AuditAnswer::Yes) {
                ctx.reject(
                    &[retry],
                    &ctx.creation_signers(),
                    ExpectedError::Protocol(Error::InvalidHistory),
                );
                continue;
            }
            let previous_case = ctx.account(ctx.case);
            let previous_vault = ctx.account(ctx.vault);
            let balance = ctx.amount(ctx.token(Actor::Author));
            ctx.ok(&[retry], &ctx.creation_signers());
            let created: tekmerion::Case = ctx.state(solana_address::Address::new_from_array(
                next.address().to_bytes(),
            ));
            assert_eq!(created.previous_case, Some(pk(ctx.case)));
            assert_eq!(
                created.mode,
                if previous_direct {
                    CaseMode::Challenge
                } else {
                    CaseMode::DirectAudit
                }
            );
            let deposit = if previous_direct { BOND } else { 7 };
            assert_eq!(created.author_principal, deposit);
            assert_eq!(ctx.amount(ctx.token(Actor::Author)), balance - deposit);
            assert_eq!(ctx.account(ctx.case), previous_case);
            assert_eq!(ctx.account(ctx.vault), previous_vault);
            // Unpaid claims stay attached to the old Case after its successor starts.
            ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
            assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
        }
    }
}

#[test]
fn direct_creation_and_sdk_share_checked_signed_clock_arithmetic() {
    let mut rules = committee_rules(1, 1);
    rules.audit_timeout = u64::MAX;
    let mut ctx = TestContext::new();
    let initial_time = ctx.clock().unix_timestamp;
    ctx.set_time(i64::MIN);
    ctx.ok(&[ctx.update_policy_ix(rules)], &[Actor::Admin]);
    ctx.set_time(initial_time);
    ctx.reject(
        &[ctx.direct_case_ix(7)],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::ArithmeticOverflow),
    );
    assert_eq!(ctx.policy_state().active_uses, 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    // The supported signed Clock and unsigned duration widen before addition.
    // A duration need not fit i64 when the final deadline does.
    ctx.set_time(i64::MIN);
    direct(&mut ctx);
    assert_eq!(ctx.case_state().audit.deadline, Some(i64::MAX));
    assert_eq!(ctx.view().case().audit.deadline, Some(i64::MAX));
}
