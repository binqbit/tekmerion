use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, START, TARGET, TestContext, committee_rules, rules,
};
use sha2::Digest;
use tekmerion::{AuditAnswer, Case, CasePhase, errors::TekmerionError as Error};

#[test]
fn funded_audit_starts_once_and_quorum_atomically_establishes_entitlements() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    assert_eq!(ctx.policy_state().rules, rules());
    let policy = ctx.account(ctx.policy);
    let immutable_inputs = |case: &Case| {
        (
            case.policy,
            case.subject,
            case.author,
            case.nonce,
            case.previous_case,
            case.assertion,
            case.author_principal,
            case.challenge_target,
            case.funding_deadline,
            case.bump,
            case.vault_bump,
        )
    };
    let initial_inputs = immutable_inputs(&ctx.case_state());
    assert_eq!(ctx.case_state().audit.deadline, None);
    assert_eq!(ctx.case_state().audit.response, None);
    ctx.contribute(Actor::Alice, 40);
    ctx.reject(
        &[ctx.finalize_timeout_ix()],
        &[],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
    ctx.set_time(START + 25);
    ctx.contribute(Actor::Bob, 100);
    let funded = ctx.case_state();
    assert_eq!(funded.challenge_principal, TARGET);
    assert_eq!(immutable_inputs(&funded), initial_inputs);
    assert_eq!(ctx.account(ctx.policy), policy);
    assert_eq!(funded.audit.deadline, Some(START + 25 + 300));
    assert_eq!(funded.phase(), CasePhase::Funded);
    assert_eq!(ctx.amount(ctx.vault), BOND + TARGET);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Bob, 1)],
        &[Actor::Bob],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    // Anchor's instruction discriminator is followed by just the decision.
    let instruction = ctx.submit_audit_ix(Actor::Auditor1, &decision);
    let mut expected = sha2::Sha256::digest(b"global:submit_audit")[..8].to_vec();
    expected.extend_from_slice(&[0, 0]); // Yes, Decisive.
    assert_eq!(instruction.data, expected);
    ctx.deliver(&decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 7);
    assert_eq!(ctx.amount(ctx.vault), BOND + TARGET - 7);
    assert_eq!(immutable_inputs(&ctx.case_state()), initial_inputs);
    assert_eq!(ctx.account(ctx.policy), policy);
}

#[test]
fn three_of_five_pays_only_matching_voters_in_configuration_order() {
    let mut ctx = TestContext::with_rules(committee_rules(3, 5), BALANCE);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    for (actor, answer) in [
        (Actor::Auditor5, AuditAnswer::No),
        (Actor::Auditor1, AuditAnswer::Yes),
        (Actor::Auditor3, AuditAnswer::No),
        (Actor::Auditor2, AuditAnswer::No),
    ] {
        ctx.vote(actor, &ctx.audit_decision(answer));
    }
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.votes.len(), 4);
    for (actor, fee) in [
        (Actor::FeeRecipient, 0),
        (Actor::FeeRecipient2, 3),
        (Actor::FeeRecipient3, 2),
        (Actor::FeeRecipient4, 0),
        (Actor::FeeRecipient5, 2),
    ] {
        assert_eq!(ctx.amount(ctx.token(actor)), BALANCE + fee);
    }
    assert_eq!(ctx.case_state().contributor_entitlement, 218);
    ctx.reject(
        &[ctx.submit_audit_ix(Actor::Auditor4, &ctx.audit_decision(AuditAnswer::No))],
        &[Actor::Auditor4],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
}

#[test]
fn maximum_committee_creation_and_nine_winner_payouts_fit_real_transactions() {
    use crate::support::{MINT, pk};
    use solana_keypair::Keypair;
    use solana_signer::Signer;
    let count = tekmerion::MAX_AUDITORS;
    assert_eq!(count, 9);
    let auditors: Vec<_> = (0..count)
        .map(|index| Keypair::new_from_array([100 + index as u8; 32]))
        .collect();
    let wallets: Vec<_> = (0..count)
        .map(|index| Keypair::new_from_array([110 + index as u8; 32]))
        .collect();
    let token_invocation = format!("Program {} invoke", crate::support::TOKEN);

    // The small fee exercises rounding and zero shares; the larger fee makes
    // every winner receive a positive transfer under the default bank budget.
    for (fee, expected_shares) in [(7, [1, 1, 1, 1, 1, 1, 1, 0, 0]), (99, [11; 9])] {
        let mut policy = rules();
        policy.threshold = count as u8;
        policy.audit_fee = fee;
        policy.auditors = auditors
            .iter()
            .zip(&wallets)
            .map(|(auditor, wallet)| tekmerion::AuditorConfig {
                authority: pk(auditor.pubkey()),
                fee_recipient: pk(wallet.pubkey()),
            })
            .collect();
        let mut ctx = TestContext::with_rules(policy, BALANCE);
        let mut recipients = Vec::new();
        for (index, wallet) in wallets.iter().enumerate() {
            ctx.fund(auditors[index].pubkey(), 1_000_000);
            recipients.push(ctx.create_token_account(130 + index as u8, wallet.pubkey(), MINT));
        }
        ctx.create_case();
        ctx.contribute(Actor::Alice, 100);
        let decision = ctx.audit_decision(AuditAnswer::Yes);
        for (index, auditor) in auditors.iter().enumerate() {
            let destinations = if index + 1 == count {
                recipients.iter().copied().map(pk).collect()
            } else {
                Vec::new()
            };
            let instruction = tekmerion_sdk::instructions::submit_audit(
                ctx.case_address(),
                pk(auditor.pubkey()),
                decision.answer,
                decision.reason,
                &destinations,
            );
            let execution = ctx
                .send(&[instruction], &[], &[auditor])
                .expect("maximum committee transaction executes within packet and compute limits");
            if index + 1 == count {
                assert!(execution.compute_units_consumed <= 200_000);
                assert_eq!(
                    execution
                        .logs
                        .iter()
                        .filter(|line| line.starts_with(&token_invocation))
                        .count(),
                    count,
                    "every winning fee destination must receive a Token CPI"
                );
            }
        }
        assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
        assert_eq!(ctx.case_state().audit.votes.len(), count);
        assert_eq!(ctx.case_state().fee_entitlement, fee);
        assert!(ctx.case_state().fee_claimed);
        assert_eq!(ctx.amount(ctx.vault), BOND + TARGET - fee);
        for (recipient, amount) in recipients.into_iter().zip(expected_shares) {
            assert_eq!(ctx.amount(recipient), amount);
        }
    }
}

#[test]
fn audit_deadline_overflow_reverts_final_credit_and_receipt_rent() {
    let mut ctx = TestContext::new();
    ctx.set_time(i64::MAX - 100);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 40);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Bob, 60)],
        &[Actor::Bob],
        ExpectedError::Protocol(Error::ArithmeticOverflow),
    );
    assert_eq!(ctx.case_state().challenge_principal, 40);
    assert_eq!(ctx.case_state().audit.deadline, None);
    assert!(ctx.get_account(&ctx.receipt(Actor::Bob)).is_none());
    ctx.finalize_optimistic_at_funding_deadline();
    assert_eq!(ctx.case_state().author_entitlement, BOND);
}
