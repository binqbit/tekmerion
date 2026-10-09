use crate::support::{
    Actor, BALANCE, ExpectedError, TestContext, account_meta_mut, committee_rules, pk,
};
use anchor_lang::error::ErrorCode;
use solana_instruction::AccountMeta;
use tekmerion::{AuditAnswer, AuditReason, CasePhase, errors::TekmerionError as Error};

fn funded_case() -> TestContext {
    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    ctx
}

#[test]
fn authorized_votes_require_full_challenge_funding_and_remain_retryable() {
    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    ctx.create_case();
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    for principal in [0, 40] {
        if principal > 0 {
            ctx.contribute(Actor::Alice, principal);
        }
        ctx.reject(
            &[ctx.submit_audit_ix(Actor::Auditor1, &decision)],
            &[Actor::Auditor1],
            ExpectedError::Protocol(Error::InvalidPhase),
        );
        let case = ctx.case_state();
        assert_eq!(case.phase(), CasePhase::Funding);
        assert_eq!(case.challenge_principal, principal);
        assert_eq!(case.audit.deadline, None);
        assert!(case.audit.votes.is_empty());
        assert_eq!(case.audit.response, None);
        assert!(!case.fee_claimed);
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    }
    ctx.contribute(Actor::Bob, 60);
    ctx.vote(Actor::Auditor1, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.response, Some(decision));
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 3);
}

#[test]
fn another_valid_policy_cannot_authorize_an_attacker_for_the_case() {
    use solana_address::Address;
    use tekmerion_sdk::instructions::{CreatePolicy, create_policy};

    let mut ctx = funded_case();
    let outsider = pk(ctx.actor(Actor::Outsider));
    let mut attacker_rules = committee_rules(1, 1);
    attacker_rules.auditors[0].authority = outsider;
    attacker_rules.auditors[0].fee_recipient = outsider;
    let identifier = 77;
    let policy = Address::new_from_array(
        tekmerion::Policy::address(&outsider, identifier)
            .0
            .to_bytes(),
    );
    // This Policy is created legitimately under the attacker's own admin key.
    ctx.ok(
        &[create_policy(CreatePolicy {
            payer: pk(ctx.actor(Actor::Payer)),
            admin: outsider,
            client_authority: outsider,
            identifier,
            rules: attacker_rules.clone(),
        })],
        &[Actor::Outsider],
    );
    ctx.protected.insert(policy);
    let stored: tekmerion::Policy = ctx.state(policy);
    assert_eq!(stored.rules, attacker_rules);
    assert!(stored.rules.has_valid_parameters());
    assert_eq!(stored.active_uses, 0);
    let decision = ctx.audit_decision(AuditAnswer::No);
    let mut attack = ctx.submit_audit_ix(Actor::Outsider, &decision);
    account_meta_mut(&mut attack, ctx.policy).pubkey = policy;
    attack
        .accounts
        .push(AccountMeta::new(ctx.token(Actor::Outsider), false));
    ctx.reject(
        &[attack],
        &[Actor::Outsider],
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );
    assert!(ctx.case_state().audit.votes.is_empty());
    assert_eq!(ctx.case_state().policy, pk(ctx.policy));
    assert_eq!(ctx.amount(ctx.token(Actor::Outsider)), BALANCE);
    ctx.vote(Actor::Auditor1, &decision);
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.response, Some(decision));
    assert_eq!(ctx.amount(ctx.token(Actor::Outsider)), BALANCE);
}

#[test]
fn auditor_signatures_membership_and_one_vote_per_identity_are_enforced() {
    let mut ctx = funded_case();
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    let mut unsigned = ctx.submit_audit_ix(Actor::Auditor1, &decision);
    account_meta_mut(&mut unsigned, ctx.actor(Actor::Auditor1)).is_signer = false;
    ctx.reject(
        &[unsigned],
        &[],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );
    ctx.reject(
        &[ctx.submit_audit_ix(Actor::Outsider, &decision)],
        &[Actor::Outsider],
        ExpectedError::Protocol(Error::UnauthorizedAuditor),
    );
    ctx.vote(Actor::Auditor1, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    for answer in [AuditAnswer::Yes, AuditAnswer::No] {
        ctx.reject(
            &[ctx.submit_audit_ix(Actor::Auditor1, &ctx.audit_decision(answer))],
            &[Actor::Auditor1],
            ExpectedError::Protocol(Error::DuplicateAuditVote),
        );
    }
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    ctx.reject(
        &[ctx.submit_audit_ix(Actor::Auditor3, &decision)],
        &[Actor::Auditor3],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
}

#[test]
fn signed_decisions_bind_case_accounts_and_validate_answer_reason() {
    use anchor_lang::AccountDeserialize;
    use solana_message::Message;
    use solana_transaction::Transaction;
    use solana_transaction_error::TransactionError;
    use tekmerion::{AuditDecision, Case};

    let mut ctx = funded_case();
    for (answer, reason) in [
        (AuditAnswer::Yes, AuditReason::Inconclusive),
        (AuditAnswer::No, AuditReason::MissingEvidence),
        (AuditAnswer::Unknown, AuditReason::Decisive),
    ] {
        let invalid = AuditDecision { answer, reason };
        ctx.reject(
            &[ctx.submit_audit_ix(Actor::Auditor1, &invalid)],
            &[Actor::Auditor1],
            ExpectedError::Protocol(Error::InvalidAuditResponse),
        );
    }

    let decision = ctx.audit_decision(AuditAnswer::Yes);
    let original_instruction = ctx.submit_audit_ix(Actor::Auditor1, &decision);
    let original_case = ctx.case;
    let original_vault = ctx.vault;
    ctx.select_fresh_subject(1);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);

    // Even a fresh signature cannot combine one Case with another Case's vault.
    let mut mismatched = original_instruction.clone();
    account_meta_mut(&mut mismatched, original_vault).pubkey = ctx.vault;
    ctx.reject(
        &[mismatched],
        &[Actor::Auditor1],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );

    ctx.new_blockhash();
    let message = Message::new_with_blockhash(
        &[original_instruction],
        Some(&ctx.actor(Actor::Payer)),
        &ctx.latest_blockhash(),
    );
    let signed = Transaction::new(
        &[
            &ctx.keys[Actor::Payer as usize],
            &ctx.keys[Actor::Auditor1 as usize],
        ],
        message,
        ctx.latest_blockhash(),
    );
    signed.verify().unwrap();
    let before: Vec<_> = ctx
        .protected
        .iter()
        .map(|key| (*key, ctx.get_account(key)))
        .collect();
    for variant in 0..3 {
        let mut changed = signed.clone();
        match variant {
            0 => {
                let index = changed
                    .message
                    .account_keys
                    .iter()
                    .position(|key| *key == original_case)
                    .unwrap();
                changed.message.account_keys[index] = ctx.case;
            }
            1 => {
                let index = changed
                    .message
                    .account_keys
                    .iter()
                    .position(|key| *key == original_vault)
                    .unwrap();
                changed.message.account_keys[index] = ctx.vault;
            }
            _ => changed.message.instructions[0].data[8] = 1, // No instead of Yes.
        }
        assert_eq!(changed.verify(), Err(TransactionError::SignatureFailure));
        // The harness verifies signatures before bank submission; BanksClient
        // executes transactions without the network signature-admission stage.
        assert_eq!(
            ctx.process_signed(changed).unwrap_err().err,
            TransactionError::SignatureFailure,
        );
        for (key, account) in &before {
            assert_eq!(ctx.get_account(key), *account);
        }
    }
    ctx.process_signed(signed).unwrap();
    let old = Case::try_deserialize(&mut &ctx.account(original_case).data[..]).unwrap();
    assert_eq!(old.audit.votes.len(), 1);
    assert_eq!(old.audit.votes[0].response, decision);
    assert!(ctx.case_state().audit.votes.is_empty());
    ctx.vote(Actor::Auditor1, &decision);
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
}

#[test]
fn matching_unknown_answers_may_have_distinct_reasons() {
    let mut ctx = funded_case();
    let mut decision = ctx.audit_decision(AuditAnswer::Unknown);
    decision.reason = AuditReason::MissingEvidence;
    ctx.vote(Actor::Auditor1, &decision);
    decision.reason = AuditReason::ConflictingEvidence;
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.votes.len(), 2);
}

#[test]
fn nonfinal_votes_accept_no_fee_accounts_and_policy_cannot_change_mid_case() {
    let mut ctx = funded_case();
    let decision = ctx.audit_decision(AuditAnswer::No);
    let mut extra = ctx.submit_audit_ix(Actor::Auditor1, &decision);
    extra
        .accounts
        .push(AccountMeta::new(ctx.token(Actor::FeeRecipient), false));
    ctx.reject(
        &[extra],
        &[Actor::Auditor1],
        ExpectedError::Protocol(Error::InvalidPayoutAccounts),
    );
    let mut replacement = committee_rules(2, 3);
    replacement.auditors[0].authority = pk(ctx.actor(Actor::Outsider));
    ctx.reject(
        &[ctx.update_policy_ix(replacement)],
        &[Actor::Admin],
        ExpectedError::Protocol(Error::PolicyInUse),
    );
}
