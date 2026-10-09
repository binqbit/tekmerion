use crate::support::{Actor, BALANCE, BOND, ExpectedError, TestContext};
use solana_program_pack::Pack;
use spl_token_interface::state::Account as TokenAccount;
use tekmerion::{AuditAnswer, CasePhase, CompletionMethod, errors::TekmerionError as Error};

#[test]
fn failed_bond_or_later_contract_instruction_rolls_back_creation_and_custody() {
    let mut ctx = TestContext::new();
    let subject_before = ctx.account(ctx.subject);
    ctx.reject(
        &[ctx.submit_case_ix(99)],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::InsufficientBond),
    );
    ctx.reject(
        &[ctx.submit_case_ix(BALANCE + 1)],
        &ctx.creation_signers(),
        ExpectedError::Custom(1),
    );
    ctx.freeze(ctx.token(Actor::Author), true);
    ctx.reject(
        &[ctx.submit_case_ix(BOND)],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    ctx.freeze(ctx.token(Actor::Author), false);
    let failure = ctx.reject_at(
        &[ctx.submit_case_ix(BOND), ctx.finalize_ix()],
        &ctx.creation_signers(),
        1,
        ExpectedError::Protocol(Error::FundingStillOpen),
    );
    assert!(
        failure
            .meta
            .logs
            .iter()
            .any(|line| line == &format!("Program {} success", crate::support::MAIN))
    );
    assert!(ctx.get_account(&ctx.case).is_none());
    assert!(ctx.get_account(&ctx.vault).is_none());
    assert!(ctx.get_account(&ctx.author_receipt()).is_none());
    assert_eq!(ctx.policy_state().active_uses, 0);
    assert_eq!(ctx.account(ctx.subject), subject_before);
    ctx.create_case();
    assert_eq!(ctx.amount(ctx.vault), BOND);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
    assert_eq!(ctx.policy_state().active_uses, 1);
}

#[test]
fn tkt_054_057_069_freeze_failures_preserve_receipts_claims_and_exact_retry_amount() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.freeze(ctx.token(Actor::Alice), true);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Alice, 23)],
        &[Actor::Alice],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    assert!(ctx.get_account(&ctx.receipt(Actor::Alice)).is_none());
    assert_eq!(ctx.case_state().unclaimed_contributors, 0);
    ctx.freeze(ctx.token(Actor::Alice), false);
    ctx.contribute(Actor::Alice, 23);
    assert_eq!(ctx.case_state().unclaimed_contributors, 1);
    ctx.freeze(ctx.token(Actor::Bob), true);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Bob, 100)],
        &[Actor::Bob],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    assert_eq!(ctx.case_state().unclaimed_contributors, 1);
    ctx.freeze(ctx.token(Actor::Bob), false);
    ctx.contribute(Actor::Bob, 7);
    assert_eq!(ctx.case_state().unclaimed_contributors, 2);
    ctx.finalize_optimistic_at_funding_deadline();
    let mut unclaimed = 2;
    for actor in [Actor::Alice, Actor::Bob, Actor::Author] {
        let claim = if matches!(actor, Actor::Author) {
            ctx.author_claim_ix()
        } else {
            ctx.contributor_claim_ix(actor)
        };
        ctx.freeze(ctx.token(actor), true);
        ctx.reject(
            std::slice::from_ref(&claim),
            &[actor],
            ExpectedError::Protocol(Error::InvalidTokenAccount),
        );
        assert_eq!(ctx.case_state().unclaimed_contributors, unclaimed);
        ctx.freeze(ctx.token(actor), false);
        ctx.ok(&[claim], &[actor]);
        if !matches!(actor, Actor::Author) {
            unclaimed -= 1;
        }
        assert_eq!(ctx.case_state().unclaimed_contributors, unclaimed);
        assert_eq!(ctx.amount(ctx.token(actor)), BALANCE);
    }
    assert_eq!(ctx.amount(ctx.vault), 0);
}

#[test]
fn tkt_054_069_late_transaction_failure_restores_already_transferred_claims() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 1);
    ctx.finalize_optimistic_at_funding_deadline();
    for (actor, claim) in [
        (Actor::Alice, ctx.contributor_claim_ix(Actor::Alice)),
        (Actor::Author, ctx.author_claim_ix()),
    ] {
        let late_failure = ctx.transfer_ix(
            ctx.token(Actor::Outsider),
            ctx.token(Actor::Bob),
            Actor::Outsider,
            BALANCE + 1,
        );
        let failure = ctx.reject_at(
            &[claim.clone(), late_failure],
            &[actor, Actor::Outsider],
            1,
            ExpectedError::Custom(1),
        );
        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|line| line == &format!("Program {} success", crate::support::MAIN)),
            "claim must have completed before the later Token failure"
        );
        ctx.ok(&[claim], &[actor]);
    }
    assert_eq!(ctx.amount(ctx.vault), 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
}

#[test]
fn tkt_054_057_069_injected_backing_deficits_preserve_finalization_and_claim_rights() {
    for (method, bob_principal, author_payout, fee) in [
        (CompletionMethod::Optimistic, 7, BOND, 0),
        (CompletionMethod::Audited, 77, 118, 7),
        (CompletionMethod::TechnicalFailure, 77, BOND, 0),
    ] {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 23);
        ctx.contribute(Actor::Bob, bob_principal);
        let finalize = match method {
            CompletionMethod::Optimistic => {
                ctx.set_time(ctx.case_state().funding_deadline);
                ctx.finalize_ix()
            }
            CompletionMethod::Audited => {
                ctx.submit_audit_ix(Actor::Auditor1, &ctx.audit_decision(AuditAnswer::Unknown))
            }
            CompletionMethod::TechnicalFailure => {
                ctx.set_time(ctx.case_state().audit.deadline.unwrap());
                ctx.finalize_timeout_ix()
            }
        };
        let finalizers = if method == CompletionMethod::Audited {
            vec![Actor::Auditor1]
        } else {
            vec![]
        };
        let phase = ctx.case_state().phase();
        let principal = BOND + 23 + bob_principal;
        assert_eq!(ctx.amount(ctx.vault), principal);
        inject_vault_deficit(&mut ctx);
        ctx.reject(
            std::slice::from_ref(&finalize),
            &finalizers,
            ExpectedError::Protocol(Error::InsufficientBacking),
        );
        assert_eq!(ctx.case_state().phase(), phase);
        assert_eq!(ctx.unpaid(), 0);
        ctx.ok(
            &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 1)],
            &[Actor::Outsider],
        );
        ctx.ok(&[finalize], &finalizers);
        assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
        assert_eq!(ctx.case_state().completion_method(), Some(method));
        assert_eq!(ctx.unpaid(), u128::from(principal - fee));

        for (actor, payout) in [(Actor::Author, author_payout), (Actor::Alice, 23)] {
            if payout == 0 {
                continue;
            }
            let claim = match actor {
                Actor::Author => ctx.author_claim_ix(),
                Actor::Alice => ctx.contributor_claim_ix(actor),
                _ => unreachable!(),
            };
            let before = ctx.case_state();
            let author_claimed_before = ctx.author_receipt_state().claimed;
            let receipt_claimed = ctx.receipt_state(Actor::Alice).claimed;
            let destination_balance = ctx.amount(ctx.token(actor));
            assert_eq!(
                u128::from(ctx.amount(ctx.vault)),
                before.unpaid(author_claimed_before)
            );
            inject_vault_deficit(&mut ctx);
            assert!(
                ctx.amount(ctx.vault) >= payout,
                "the individual transfer is affordable; aggregate liabilities are not"
            );
            ctx.reject(
                std::slice::from_ref(&claim),
                &[actor],
                ExpectedError::Protocol(Error::InsufficientBacking),
            );
            let rejected = ctx.case_state();
            assert_eq!(ctx.author_receipt_state().claimed, author_claimed_before);
            assert_eq!(rejected.fee_claimed, before.fee_claimed);
            assert_eq!(rejected.contributor_unpaid, before.contributor_unpaid);
            assert_eq!(
                rejected.unclaimed_contributors,
                before.unclaimed_contributors
            );
            assert_eq!(
                rejected.unpaid(ctx.author_receipt_state().claimed),
                before.unpaid(author_claimed_before)
            );
            assert_eq!(ctx.receipt_state(Actor::Alice).claimed, receipt_claimed);
            ctx.ok(
                &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 1)],
                &[Actor::Outsider],
            );
            ctx.ok(&[claim], &[actor]);
            let paid = ctx.case_state();
            assert_eq!(ctx.amount(ctx.token(actor)), destination_balance + payout);
            assert_eq!(
                paid.unpaid(ctx.author_receipt_state().claimed),
                before.unpaid(author_claimed_before) - u128::from(payout)
            );
            assert_eq!(
                paid.unclaimed_contributors,
                before.unclaimed_contributors - u64::from(matches!(actor, Actor::Alice))
            );
            assert_eq!(
                u128::from(ctx.amount(ctx.vault)),
                paid.unpaid(ctx.author_receipt_state().claimed)
            );
        }
        assert_eq!(ctx.case_state().unclaimed_contributors, 1);
        ctx.ok(&[ctx.contributor_claim_ix(Actor::Bob)], &[Actor::Bob]);
        assert_eq!(ctx.amount(ctx.token(Actor::Bob)), BALANCE);
        assert_eq!(ctx.case_state().unclaimed_contributors, 0);
        assert_eq!(ctx.unpaid(), 0);
        assert_eq!(ctx.amount(ctx.vault), 0);
    }
}

fn inject_vault_deficit(ctx: &mut TestContext) {
    // Synthetic corruption tests the fail-closed custody invariant. A normal
    // transaction cannot withdraw from this PDA-owned vault without the program.
    let mut vault = ctx.account(ctx.vault);
    let mut token = TokenAccount::unpack(&vault.data).unwrap();
    token.amount = token.amount.checked_sub(1).unwrap();
    TokenAccount::pack(token, &mut vault.data).unwrap();
    ctx.set_account(ctx.vault, vault);
}

#[test]
fn critical_funding_deficits_cannot_be_hidden_by_new_stakes_or_topups() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    for (actor, requested, accepted, total, contributors) in [
        (Actor::Alice, 23, 23, 23, 1),
        (Actor::Alice, 7, 7, 30, 1),
        (Actor::Bob, u64::MAX, 70, 100, 2),
    ] {
        let contribution = ctx.contribute_ix(actor, requested);
        let receipt_before = ctx.get_account(&ctx.receipt(actor));
        let source_before = ctx.amount(ctx.token(actor));
        let vault_before = ctx.amount(ctx.vault);
        let principal_before = ctx.case_state().challenge_principal;
        inject_vault_deficit(&mut ctx);
        assert!(ctx.amount(ctx.vault) + accepted >= vault_before);
        ctx.reject(
            std::slice::from_ref(&contribution),
            &[actor],
            ExpectedError::Protocol(Error::InsufficientBacking),
        );
        assert_eq!(ctx.get_account(&ctx.receipt(actor)), receipt_before);
        assert_eq!(ctx.case_state().challenge_principal, principal_before);
        assert_eq!(ctx.case_state().audit.deadline, None);
        ctx.ok(
            &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 1)],
            &[Actor::Outsider],
        );
        assert_eq!(ctx.amount(ctx.vault), vault_before);
        ctx.ok(&[contribution], &[actor]);
        assert_eq!(ctx.amount(ctx.token(actor)), source_before - accepted);
        assert_eq!(ctx.amount(ctx.vault), vault_before + accepted);
        assert_eq!(ctx.case_state().challenge_principal, total);
        assert_eq!(ctx.case_state().unclaimed_contributors, contributors);
        assert_eq!(
            ctx.receipt_state(actor).principal,
            if total == 100 { 70 } else { total }
        );
    }
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(
        ctx.case_state().audit.deadline,
        Some(ctx.clock().unix_timestamp + ctx.policy_state().rules.audit_timeout as i64)
    );
}

#[test]
fn critical_zero_claims_require_backing_and_valid_destinations_before_discharge() {
    for (answer, actor) in [
        (AuditAnswer::Yes, Actor::Alice),
        (AuditAnswer::No, Actor::Author),
    ] {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 100);
        ctx.deliver(&ctx.audit_decision(answer));
        let claim = if matches!(actor, Actor::Author) {
            assert_eq!(ctx.case_state().author_entitlement, 0);
            ctx.author_claim_ix()
        } else {
            assert_eq!(ctx.case_state().contributor_claim(100).unwrap(), 0);
            ctx.contributor_claim_ix(actor)
        };
        let receipt = if matches!(actor, Actor::Author) {
            ctx.author_receipt()
        } else {
            ctx.receipt(actor)
        };
        let before = ctx.case_without_claim_progress();
        let unpaid = ctx.unpaid();
        let vault_balance = ctx.amount(ctx.vault);
        let destination_balance = ctx.amount(ctx.token(actor));
        let contributor_count = ctx.case_state().unclaimed_contributors;
        assert!(unpaid > 0);
        inject_vault_deficit(&mut ctx);
        ctx.reject(
            std::slice::from_ref(&claim),
            &[actor],
            ExpectedError::Protocol(Error::InsufficientBacking),
        );
        assert!(!ctx.state::<tekmerion::StakeReceipt>(receipt).claimed);
        ctx.ok(
            &[ctx.transfer_ix(ctx.token(Actor::Outsider), ctx.vault, Actor::Outsider, 1)],
            &[Actor::Outsider],
        );
        ctx.freeze(ctx.token(actor), true);
        ctx.reject(
            std::slice::from_ref(&claim),
            &[actor],
            ExpectedError::Protocol(Error::InvalidTokenAccount),
        );
        assert!(!ctx.state::<tekmerion::StakeReceipt>(receipt).claimed);
        assert_eq!(ctx.unpaid(), unpaid);
        ctx.freeze(ctx.token(actor), false);
        ctx.ok(std::slice::from_ref(&claim), &[actor]);
        assert!(ctx.state::<tekmerion::StakeReceipt>(receipt).claimed);
        assert_eq!(ctx.amount(ctx.vault), vault_balance);
        assert_eq!(ctx.amount(ctx.token(actor)), destination_balance);
        assert_eq!(ctx.unpaid(), unpaid);
        assert_eq!(ctx.case_without_claim_progress(), before);
        assert_eq!(
            ctx.case_state().unclaimed_contributors,
            contributor_count - u64::from(matches!(actor, Actor::Alice))
        );
        ctx.reject(
            &[claim],
            &[actor],
            ExpectedError::Protocol(Error::AlreadyClaimed),
        );
    }
}

#[test]
fn critical_vault_metadata_rejects_claims_without_consuming_receipts() {
    for variant in 0..3 {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 10);
        ctx.finalize_optimistic_at_funding_deadline();
        let clean_vault = ctx.account(ctx.vault);
        let destination_before = ctx.amount(ctx.token(Actor::Author));
        let author_receipt_before = ctx.account(ctx.author_receipt());
        // These mutations deliberately corrupt a PDA-owned vault. The program
        // exposes no instruction granting these token-account authorities.
        let mut corrupted = clean_vault.clone();
        let mut token = TokenAccount::unpack(&corrupted.data).unwrap();
        match variant {
            0 => {
                token.delegate = Some(crate::support::pk(ctx.actor(Actor::Outsider))).into();
                token.delegated_amount = 1;
            }
            1 => {
                token.close_authority = Some(crate::support::pk(ctx.actor(Actor::Outsider))).into()
            }
            _ => token.is_native = Some(1).into(),
        }
        TokenAccount::pack(token, &mut corrupted.data).unwrap();
        ctx.set_account(ctx.vault, corrupted);
        let claim = ctx.author_claim_ix();
        ctx.reject(
            std::slice::from_ref(&claim),
            &[Actor::Author],
            ExpectedError::Protocol(Error::InvalidTokenAccount),
        );
        assert_eq!(ctx.account(ctx.author_receipt()), author_receipt_before);
        ctx.set_account(ctx.vault, clean_vault);
        ctx.ok(&[claim], &[Actor::Author]);
        assert!(ctx.author_receipt_state().claimed);
        assert_eq!(
            ctx.amount(ctx.token(Actor::Author)),
            destination_before + BOND
        );
        assert_eq!(ctx.amount(ctx.vault), 10);
        assert_eq!(ctx.unpaid(), 10);
        ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
        assert_eq!(ctx.amount(ctx.vault), 0);
    }
}

#[test]
fn tkt_059_invalid_rules_and_deadline_overflow_leave_no_exposure() {
    let mut ctx = TestContext::new();
    for field in 0..6 {
        let mut rules = crate::support::rules();
        match field {
            0 => rules.author_bond = 0,
            1 => rules.challenge_target = 0,
            2 => rules.funding_duration = 0,
            3 => rules.audit_timeout = 0,
            4 => rules.author_bond = rules.audit_fee - 1,
            _ => rules.challenge_target = rules.audit_fee - 1,
        }
        ctx.reject(
            &[ctx.update_policy_ix(rules)],
            &[Actor::Admin],
            ExpectedError::Protocol(Error::InvalidRules),
        );
    }
    for invalid in 0..7 {
        let mut rules = crate::support::committee_rules(2, 3);
        match invalid {
            0 => rules.threshold = 0,
            1 => rules.threshold = 1,
            2 => rules.threshold = 4,
            3 => rules.auditors[0].authority = anchor_lang::prelude::Pubkey::default(),
            4 => rules.auditors[0].fee_recipient = anchor_lang::prelude::Pubkey::default(),
            5 => rules.auditors[1].authority = rules.auditors[0].authority,
            _ => rules.auditors[1].fee_recipient = rules.auditors[0].fee_recipient,
        }
        ctx.reject(
            &[ctx.update_policy_ix(rules)],
            &[Actor::Admin],
            ExpectedError::Protocol(Error::InvalidRules),
        );
    }
    ctx.ok(
        &[ctx.update_policy_ix(crate::support::rules())],
        &[Actor::Admin],
    );
    let mut oversized = crate::support::rules();
    oversized.challenge_target = u64::MAX;
    ctx.ok(&[ctx.update_policy_ix(oversized)], &[Actor::Admin]);
    ctx.reject(
        &[ctx.submit_case_ix(BOND)],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::ArithmeticOverflow),
    );
    assert!(ctx.get_account(&ctx.case).is_none());
    assert!(ctx.get_account(&ctx.vault).is_none());
    assert!(ctx.get_account(&ctx.author_receipt()).is_none());
    assert_eq!(ctx.policy_state().active_uses, 0);
    ctx.ok(
        &[ctx.update_policy_ix(crate::support::rules())],
        &[Actor::Admin],
    );
    ctx.set_time(i64::MAX - 30);
    ctx.reject(
        &[ctx.submit_case_ix(BOND)],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::ArithmeticOverflow),
    );
    ctx.set_time(crate::support::START);
    ctx.create_case();
    assert_eq!(
        ctx.case_state().funding_deadline,
        crate::support::START + crate::support::DURATION as i64
    );
}
