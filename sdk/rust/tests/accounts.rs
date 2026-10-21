#![cfg(feature = "host")]

use anchor_lang::{
    AccountSerialize, AnchorSerialize, Discriminator, solana_program::program_pack::Pack,
};
use anchor_spl::token::{
    self,
    spl_token::state::{Account as TokenAccount, AccountState},
};
use tekmerion_sdk::{
    ID, Pubkey, SdkError, USDC_MINT,
    accounts::{AccountData, AuditPayout, CaseView, Observation, PolicyView},
    instructions::CaseAddress,
    program::{
        AUTHOR_SIDE, AuditAnswer, AuditContext, AuditDecision, AuditReason, AuditVote,
        AuditorConfig, Case, CaseMode, CasePhase, CaseResult, Policy, PolicyRules, StakeReceipt,
    },
};

const OBSERVATION: Observation = Observation {
    slot: 42,
    unix_timestamp: 150,
};

fn policy(count: usize, threshold: u8, fee: u64) -> Policy {
    let admin = Pubkey::new_from_array([20; 32]);
    Policy {
        admin,
        identifier: 8,
        client_authority: Pubkey::new_from_array([21; 32]),
        rules: PolicyRules {
            auditors: (0..count)
                .map(|index| AuditorConfig {
                    authority: Pubkey::new_from_array([30 + index as u8; 32]),
                    fee_recipient: Pubkey::new_from_array([60 + index as u8; 32]),
                })
                .collect(),
            threshold,
            author_bond: 125,
            challenge_target: 100,
            audit_fee: fee,
            funding_duration: 100,
            audit_timeout: 100,
        },
        active_uses: 0,
        bump: Policy::address(&admin, 8).1,
    }
}

fn encoded(account: &impl AccountSerialize, space: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    account.try_serialize(&mut bytes).unwrap();
    assert!(bytes.len() <= space);
    bytes.resize(space, 0);
    bytes
}

fn account(key: Pubkey, owner: Pubkey, data: &[u8]) -> AccountData<'_> {
    AccountData {
        key,
        owner,
        executable: false,
        data,
        slot: OBSERVATION.slot,
    }
}

fn vote(index: u8, answer: AuditAnswer, reason: AuditReason) -> AuditVote {
    AuditVote {
        auditor_index: index,
        response: AuditDecision { answer, reason },
    }
}

// Serialize observed account state in its documented field groups. The reader
// must decode it through the real program types; tests cannot construct Case's
// private phase/result fields or bypass CaseView's custody/lifecycle checks.
fn case_view(
    policy: &Policy,
    phase: CasePhase,
    votes: Vec<AuditVote>,
    unix_timestamp: i64,
) -> CaseView {
    read_case_view(
        policy,
        phase,
        votes,
        unix_timestamp,
        account(Pubkey::new_from_array([22; 32]), Pubkey::default(), &[]),
    )
    .unwrap()
}

fn read_case_view<'subject>(
    policy: &Policy,
    phase: CasePhase,
    votes: Vec<AuditVote>,
    unix_timestamp: i64,
    subject_account: impl Into<Option<AccountData<'subject>>>,
) -> tekmerion_sdk::Result<CaseView> {
    let policy_key = Policy::address(&policy.admin, policy.identifier).0;
    let subject = Pubkey::new_from_array([22; 32]);
    let author = Pubkey::new_from_array([23; 32]);
    let identity = CaseAddress {
        policy: policy_key,
        subject,
        author,
        nonce: 0,
    };
    let (case_key, case_bump) = Case::address(&policy_key, &subject, &author, 0);
    let (vault_key, vault_bump) = Case::vault_address(&case_key);
    let funded = phase != CasePhase::Funding;
    let finalized = phase == CasePhase::Finalized;
    let challenge_principal = if funded { 100_u64 } else { 0 };
    let audit = AuditContext {
        deadline: funded.then_some(200),
        votes,
        response: None,
    };
    let mut case_bytes = Case::DISCRIMINATOR.to_vec();
    (
        policy_key,
        subject,
        author,
        0_u64,
        None::<Pubkey>,
        [0xa1_u8; 32],
        [91_u8; 32],
        CaseMode::Challenge,
    )
        .serialize(&mut case_bytes)
        .unwrap();
    (125_u64, 100_u64, challenge_principal, 100_i64)
        .serialize(&mut case_bytes)
        .unwrap();
    audit.serialize(&mut case_bytes).unwrap();
    (phase, None::<CaseResult>, finalized.then_some(200_i64))
        .serialize(&mut case_bytes)
        .unwrap();
    (
        if finalized { 125_u64 } else { 0 },
        if finalized { 100_u128 } else { 0 },
        0_u64,
    )
        .serialize(&mut case_bytes)
        .unwrap();
    (
        if finalized { 100_u128 } else { 0 },
        u64::from(funded),
        false,
    )
        .serialize(&mut case_bytes)
        .unwrap();
    (case_bump, vault_bump).serialize(&mut case_bytes).unwrap();
    assert!(case_bytes.len() <= Case::SPACE);
    case_bytes.resize(Case::SPACE, 0);

    let receipt = StakeReceipt {
        case: case_key,
        owner: author,
        side: AUTHOR_SIDE,
        principal: 125,
        claimed: false,
        bump: StakeReceipt::address(&case_key, &author).1,
    };
    let receipt_bytes = encoded(&receipt, StakeReceipt::SPACE);
    let policy_bytes = encoded(policy, Policy::SPACE);
    let mut vault_bytes = vec![0; TokenAccount::LEN];
    TokenAccount::pack(
        TokenAccount {
            mint: USDC_MINT,
            owner: case_key,
            amount: 225,
            state: AccountState::Initialized,
            ..TokenAccount::default()
        },
        &mut vault_bytes,
    )
    .unwrap();

    CaseView::read(
        Observation {
            unix_timestamp,
            ..OBSERVATION
        },
        identity,
        account(case_key, ID, &case_bytes),
        account(policy_key, ID, &policy_bytes),
        subject_account,
        account(vault_key, token::ID, &vault_bytes),
        account(identity.receipt(&author), ID, &receipt_bytes),
    )
}

#[test]
fn optional_subject_preserves_identity_checks_when_supplied() {
    let mut policy = policy(3, 2, 7);
    policy.active_uses = 1;
    let subject = account(Pubkey::new_from_array([22; 32]), Pubkey::default(), &[]);
    let read = |subject| read_case_view(&policy, CasePhase::Funded, vec![], 150, subject);

    let without_subject = read(None).unwrap();
    let with_subject = read(Some(subject)).unwrap();
    assert_eq!(without_subject.address(), with_subject.address());
    assert_eq!(
        without_subject.vault_balance(),
        with_subject.vault_balance()
    );
    assert_eq!(without_subject.author_claim(), with_subject.author_claim());

    let mut wrong_key = subject;
    wrong_key.key = Pubkey::new_from_array([99; 32]);
    assert_eq!(
        read(Some(wrong_key)).unwrap_err(),
        SdkError::InvalidRelationship
    );
    let mut stale = subject;
    stale.slot -= 1;
    assert_eq!(
        read(Some(stale)).unwrap_err(),
        SdkError::InconsistentObservation
    );
    let mut executable = subject;
    executable.executable = true;
    assert_eq!(
        read(Some(executable)).unwrap_err(),
        SdkError::InvalidAccount
    );
}

#[test]
fn policy_can_be_read_before_any_case_and_without_new_deadline_admission() {
    let policy = policy(3, 2, 7);
    let key = Policy::address(&policy.admin, policy.identifier).0;
    let bytes = encoded(&policy, Policy::SPACE);
    let observation = Observation {
        unix_timestamp: i64::MAX,
        ..OBSERVATION
    };

    let view = PolicyView::read(observation, key, account(key, ID, &bytes)).unwrap();

    assert_eq!(view.address(), key);
    assert_eq!(view.observation(), observation);
    assert_eq!(view.policy().active_uses, 0);
    assert_eq!(view.policy().rules, policy.rules);
    assert!(view.policy().rules.validate_at(i64::MAX).is_err());
}

#[test]
fn policy_read_rejects_untrusted_metadata_identity_and_rules() {
    for variant in 0..11 {
        let mut policy = policy(3, 2, 7);
        let key = Policy::address(&policy.admin, policy.identifier).0;
        let foreign = Pubkey::new_from_array([99; 32]);
        match variant {
            4 => policy.bump ^= 1,
            5 => policy.admin = foreign,
            6 => policy.rules.threshold = 1,
            7 => policy.rules.auditors[1] = policy.rules.auditors[0].clone(),
            _ => {}
        }
        let mut bytes = encoded(&policy, Policy::SPACE);
        if variant == 8 {
            bytes.pop();
        }
        if variant == 9 {
            bytes[0] ^= 1;
        }
        let mut input = account(key, ID, &bytes);
        match variant {
            0 => input.owner = foreign,
            1 => input.executable = true,
            2 => input.slot -= 1,
            3 => input.key = foreign,
            _ => {}
        }
        let expected = if variant == 10 { foreign } else { key };
        let expected_error = match variant {
            0 | 1 | 8 | 9 => SdkError::InvalidAccount,
            2 => SdkError::InconsistentObservation,
            3..=5 | 10 => SdkError::InvalidRelationship,
            _ => SdkError::InvalidState,
        };

        let result = PolicyView::read(OBSERVATION, expected, input);

        assert_eq!(result.unwrap_err(), expected_error, "variant {variant}");
    }
}

#[test]
fn audit_quote_supports_one_and_two_vote_quorums_without_mutating_progress() {
    for (count, threshold, votes, signer, expected) in [
        (1, 1, vec![], 0, vec![(0, 7)]),
        (
            3,
            2,
            vec![vote(2, AuditAnswer::Yes, AuditReason::Decisive)],
            0,
            vec![(0, 4), (2, 3)],
        ),
    ] {
        let mut policy = policy(count, threshold, 7);
        policy.active_uses = 1;
        let view = case_view(&policy, CasePhase::Funded, votes.clone(), 199);

        let payouts = view
            .quote_audit(
                policy.rules.auditors[signer].authority,
                AuditAnswer::Yes,
                AuditReason::Decisive,
            )
            .unwrap();

        let expected: Vec<_> = expected
            .into_iter()
            .map(|(auditor_index, amount)| AuditPayout {
                auditor_index,
                fee_recipient: policy.rules.auditors[usize::from(auditor_index)].fee_recipient,
                amount,
            })
            .collect();
        assert_eq!(payouts, expected);
        assert_eq!(view.case().audit.votes, votes);
        assert_eq!(view.case().audit.response, None);
        assert_eq!(view.case().phase(), CasePhase::Funded);
    }
}

#[test]
fn audit_quote_orders_three_winners_and_keeps_every_zero_fee_destination() {
    for (fee, amounts) in [(7, [3, 2, 2]), (2, [1, 1, 0]), (0, [0, 0, 0])] {
        let mut policy = policy(5, 3, fee);
        policy.active_uses = 1;
        let votes = vec![
            vote(4, AuditAnswer::Unknown, AuditReason::MissingEvidence),
            vote(0, AuditAnswer::No, AuditReason::Decisive),
            vote(2, AuditAnswer::Unknown, AuditReason::ConflictingEvidence),
        ];
        let view = case_view(&policy, CasePhase::Funded, votes, 150);

        let payouts = view
            .quote_audit(
                policy.rules.auditors[1].authority,
                AuditAnswer::Unknown,
                AuditReason::Inconclusive,
            )
            .unwrap();
        let below_quorum = view
            .quote_audit(
                policy.rules.auditors[3].authority,
                AuditAnswer::Yes,
                AuditReason::Decisive,
            )
            .unwrap();

        assert_eq!(payouts.len(), 3);
        for ((payout, index), amount) in payouts.iter().zip([1, 2, 4]).zip(amounts) {
            assert_eq!(payout.auditor_index, index);
            assert_eq!(
                payout.fee_recipient,
                policy.rules.auditors[usize::from(index)].fee_recipient
            );
            assert_eq!(payout.amount, amount);
        }
        assert_eq!(payouts.iter().map(|payout| payout.amount).sum::<u64>(), fee);
        assert!(below_quorum.is_empty());
    }
}

#[test]
fn audit_quote_rejects_wrong_phase_deadline_identity_duplicates_and_invalid_decisions() {
    let mut policy = policy(3, 2, 7);
    policy.active_uses = 1;
    let member = policy.rules.auditors[1].authority;
    let votes = vec![vote(0, AuditAnswer::Yes, AuditReason::Decisive)];
    let view = case_view(&policy, CasePhase::Funded, votes.clone(), 199);

    for (auditor, answer, reason) in [
        (
            Pubkey::new_from_array([99; 32]),
            AuditAnswer::Yes,
            AuditReason::Decisive,
        ),
        (
            policy.rules.auditors[0].authority,
            AuditAnswer::Yes,
            AuditReason::Decisive,
        ),
        (
            policy.rules.auditors[0].authority,
            AuditAnswer::No,
            AuditReason::Decisive,
        ),
        (member, AuditAnswer::Yes, AuditReason::MissingEvidence),
        (member, AuditAnswer::Unknown, AuditReason::Decisive),
    ] {
        assert_eq!(
            view.quote_audit(auditor, answer, reason),
            Err(SdkError::ActionUnavailable)
        );
    }

    for (phase, timestamp, votes) in [
        (CasePhase::Funding, 99, vec![]),
        (CasePhase::Funded, 200, votes.clone()),
        (CasePhase::Funded, 201, votes.clone()),
        (CasePhase::Finalized, 200, votes),
    ] {
        let view = case_view(&policy, phase, votes, timestamp);

        assert_eq!(
            view.quote_audit(member, AuditAnswer::Yes, AuditReason::Decisive),
            Err(SdkError::ActionUnavailable)
        );
    }
}
