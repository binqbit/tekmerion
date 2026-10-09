use anchor_lang::AccountSerialize;
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tekmerion::{AuditAnswer, CasePhase, CaseResult, CompletionMethod};
use tekmerion_sdk::{
    SdkError,
    accounts::{CaseView, ClaimState, Observation},
};

use crate::support::{Actor, BALANCE, BOND, TestContext, observed_account, pk};

fn contributor(ctx: &TestContext, actor: Actor) -> ClaimState {
    let view = ctx.view();
    view.contributor_claim(
        pk(ctx.actor(actor)),
        observed_account(
            ctx.receipt(actor),
            &ctx.account(ctx.receipt(actor)),
            view.observation().slot,
        ),
    )
    .unwrap()
}

fn bind_subject(ctx: &mut TestContext, subject: Address) {
    ctx.subject = subject;
    ctx.case = Address::new_from_array(ctx.case_address().address().to_bytes());
    ctx.vault = Address::new_from_array(ctx.case_address().vault().to_bytes());
    ctx.protected
        .extend([ctx.subject, ctx.case, ctx.vault, ctx.author_receipt()]);
}

#[test]
fn absent_subject_remains_readable_through_audited_settlement_and_claims() {
    let mut ctx = TestContext::new();
    let subject = Keypair::new_from_array([0xe7; 32]).pubkey();
    assert!(ctx.get_account(&subject).is_none());
    bind_subject(&mut ctx, subject);

    ctx.create_case();
    let view = ctx.view();
    assert_eq!(view.case().subject, pk(subject));
    assert_eq!(view.case().phase(), CasePhase::Funding);
    assert_eq!(view.vault_balance(), BOND);
    assert_eq!(view.author_claim(), ClaimState::Unavailable);
    assert_eq!(view.quote_contribution(100).unwrap(), 100);
    assert!(ctx.get_account(&subject).is_none());

    ctx.contribute(Actor::Alice, 100);
    assert_eq!(ctx.view().case().phase(), CasePhase::Funded);
    assert_eq!(contributor(&ctx, Actor::Alice), ClaimState::Unavailable);
    ctx.deliver(&ctx.audit_decision(AuditAnswer::Unknown));

    let view = ctx.view();
    assert_eq!(view.case().result(), Some(CaseResult::Unknown));
    assert_eq!(
        view.case().completion_method(),
        Some(CompletionMethod::Audited)
    );
    assert_eq!(
        view.author_claim(),
        ClaimState::Unclaimed { amount: BOND - 7 }
    );
    assert_eq!(view.audit_fees_paid(), Some(7));
    assert_eq!(
        contributor(&ctx, Actor::Alice),
        ClaimState::Unclaimed { amount: 100 }
    );

    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
    assert_eq!(ctx.view().vault_balance(), 100);
    assert_eq!(
        contributor(&ctx, Actor::Alice),
        ClaimState::Unclaimed { amount: 100 }
    );
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);

    assert_eq!(contributor(&ctx, Actor::Alice), ClaimState::Claimed);
    assert_eq!(ctx.view().vault_balance(), 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 7);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 7);
    assert!(ctx.author_receipt_state().claimed);
    assert!(ctx.receipt_state(Actor::Alice).claimed);
    assert!(ctx.get_account(&subject).is_none());
}

#[test]
fn signed_subject_closure_preserves_views_and_optimistic_refunds() {
    let mut ctx = TestContext::new();
    let subject = Keypair::new_from_array([0xe8; 32]);
    ctx.fund(subject.pubkey(), 1_000_000);
    bind_subject(&mut ctx, subject.pubkey());
    ctx.create_case();
    ctx.contribute(Actor::Alice, 40);
    let case_before = ctx.account(ctx.case);
    let vault_before = ctx.account(ctx.vault);
    let view = ctx.view();
    assert_eq!(view.case().phase(), CasePhase::Funding);
    assert_eq!(view.quote_contribution(100).unwrap(), 60);

    // A genuine owner-signed System transfer closes the subject; no synthetic
    // replacement of its bank account or the protocol state is involved.
    let close = solana_system_interface::instruction::transfer(
        &subject.pubkey(),
        &ctx.actor(Actor::Outsider),
        ctx.account(subject.pubkey()).lamports,
    );
    ctx.send(&[close], &[], &[&subject]).unwrap();

    assert!(ctx.get_account(&subject.pubkey()).is_none());
    assert_eq!(ctx.account(ctx.case), case_before);
    assert_eq!(ctx.account(ctx.vault), vault_before);
    assert_eq!(ctx.view().case().subject, view.case().subject);
    assert_eq!(ctx.view().case().phase(), view.case().phase());
    assert_eq!(ctx.view().quote_contribution(100).unwrap(), 60);
    assert_eq!(ctx.view().author_claim(), ClaimState::Unavailable);

    ctx.finalize_optimistic_at_funding_deadline();
    let view = ctx.view();
    assert_eq!(view.case().result(), Some(CaseResult::Yes));
    assert_eq!(
        view.case().completion_method(),
        Some(CompletionMethod::Optimistic)
    );
    assert_eq!(view.author_claim(), ClaimState::Unclaimed { amount: BOND });
    assert_eq!(view.audit_fees_paid(), None);
    assert_eq!(
        contributor(&ctx, Actor::Alice),
        ClaimState::Unclaimed { amount: 40 }
    );

    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
    assert_eq!(contributor(&ctx, Actor::Alice), ClaimState::Claimed);
    assert_eq!(ctx.view().vault_balance(), BOND);
    assert_eq!(
        ctx.view().author_claim(),
        ClaimState::Unclaimed { amount: BOND }
    );
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);

    assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
    assert_eq!(ctx.view().vault_balance(), 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    assert!(ctx.author_receipt_state().claimed);
    assert!(ctx.receipt_state(Actor::Alice).claimed);
    assert!(ctx.get_account(&subject.pubkey()).is_none());
}

#[test]
fn sdk_reads_current_quotes_and_independent_claims() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    let old_quote = ctx.view().quote_contribution(100).unwrap();
    assert_eq!(old_quote, 100);
    assert_eq!(ctx.view().author_claim(), ClaimState::Unavailable);
    ctx.contribute(Actor::Alice, 20);
    assert_eq!(ctx.view().quote_contribution(100).unwrap(), 80);
    assert_eq!(contributor(&ctx, Actor::Alice), ClaimState::Unavailable);
    let before = ctx.amount(ctx.token(Actor::Bob));
    ctx.contribute(Actor::Bob, old_quote);
    assert_eq!(
        before - ctx.amount(ctx.token(Actor::Bob)),
        80,
        "a quote is not accepted credit"
    );
    assert_eq!(
        ctx.view().quote_contribution(1),
        Err(SdkError::ActionUnavailable)
    );

    ctx.deliver(&ctx.audit_decision(AuditAnswer::No));
    assert_eq!(ctx.view().case().result(), Some(CaseResult::No));
    assert_eq!(
        ctx.view().author_claim(),
        ClaimState::Unclaimed { amount: 0 }
    );
    assert_eq!(ctx.view().audit_fees_paid(), Some(7));
    let alice_quote = contributor(&ctx, Actor::Alice);
    assert_eq!(alice_quote, ClaimState::Unclaimed { amount: 43 });
    assert_eq!(
        contributor(&ctx, Actor::Bob),
        ClaimState::Unclaimed { amount: 174 }
    );

    let before = ctx.amount(ctx.token(Actor::Bob));
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Bob)], &[Actor::Bob]);
    assert_eq!(ctx.amount(ctx.token(Actor::Bob)) - before, 174);
    assert_eq!(contributor(&ctx, Actor::Bob), ClaimState::Claimed);

    // The last open receipt now owns the rounding remainder. Its old quote is
    // stale; the already paid receipt exposes no fabricated historical amount.
    assert_eq!(
        contributor(&ctx, Actor::Alice),
        ClaimState::Unclaimed { amount: 44 }
    );
    assert_ne!(contributor(&ctx, Actor::Alice), alice_quote);
    let before = ctx.amount(ctx.token(Actor::Alice));
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)) - before, 44);
    assert_eq!(contributor(&ctx, Actor::Alice), ClaimState::Claimed);
    assert_eq!(contributor(&ctx, Actor::Bob), ClaimState::Claimed);
    assert_eq!(ctx.view().case().unclaimed_contributors, 0);
    assert_eq!(
        ctx.view().author_claim(),
        ClaimState::Unclaimed { amount: 0 }
    );
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
    assert_eq!(ctx.view().audit_fees_paid(), Some(7));
    assert_eq!(ctx.view().vault_balance(), 0);
}

#[test]
fn sdk_distinguishes_unknown_from_technical_timeout() {
    for answer in [Some(AuditAnswer::Unknown), None] {
        let mut ctx = TestContext::new();
        ctx.create_case();
        ctx.contribute(Actor::Alice, 100);
        if let Some(answer) = answer {
            ctx.deliver(&ctx.audit_decision(answer));
        } else {
            ctx.set_time(ctx.case_state().audit.deadline.unwrap());
            ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
        }
        let view = ctx.view();
        let (result, method, author, fee) = if answer.is_some() {
            (Some(CaseResult::Unknown), CompletionMethod::Audited, 118, 7)
        } else {
            (None, CompletionMethod::TechnicalFailure, 125, 0)
        };
        assert_eq!(view.case().result(), result);
        assert_eq!(view.case().completion_method(), Some(method));
        assert_eq!(
            view.author_claim(),
            ClaimState::Unclaimed { amount: author }
        );
        assert_eq!(view.audit_fees_paid(), answer.map(|_| fee));
        assert_eq!(
            contributor(&ctx, Actor::Alice),
            ClaimState::Unclaimed { amount: 100 }
        );
    }
}

fn replace_state(account: &mut Account, state: &impl AccountSerialize) {
    account.data.fill(0);
    state
        .try_serialize(&mut account.data.as_mut_slice())
        .unwrap();
}

#[test]
fn sdk_rejects_mixed_observations_substitutions_and_corrupt_rights() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 20);
    ctx.finalize_optimistic_at_funding_deadline();
    let clock: Clock = ctx.clock();
    let observation = Observation {
        slot: clock.slot,
        unix_timestamp: clock.unix_timestamp,
    };
    let original = [
        ctx.case,
        ctx.policy,
        ctx.subject,
        ctx.vault,
        ctx.author_receipt(),
    ]
    .map(|key| ctx.account(key));
    let read = |accounts: &[Account; 5]| {
        CaseView::read(
            observation,
            ctx.case_address(),
            observed_account(ctx.case, &accounts[0], clock.slot),
            observed_account(ctx.policy, &accounts[1], clock.slot),
            observed_account(ctx.subject, &accounts[2], clock.slot),
            observed_account(ctx.vault, &accounts[3], clock.slot),
            observed_account(ctx.author_receipt(), &accounts[4], clock.slot),
        )
    };
    assert!(read(&original).is_ok());
    // Synthetic read-only projection: an authority need not follow this
    // client's PDA convention. This does not mutate the runtime ledger.
    let mut wallet_authority = original.clone();
    let mut wallet_policy = ctx.policy_state();
    wallet_policy.client_authority = pk(ctx.actor(Actor::Outsider));
    replace_state(&mut wallet_authority[1], &wallet_policy);
    assert_eq!(
        read(&wallet_authority).unwrap().policy().client_authority,
        wallet_policy.client_authority,
    );
    // Subject ownership and contents belong to the client adapter. The generic
    // view accepts another client's opaque schema at the bound subject address.
    let mut opaque_subject = original.clone();
    opaque_subject[2].owner = ctx.actor(Actor::Outsider);
    opaque_subject[2].data = vec![0x51, 0x72, 0x93];
    assert_eq!(
        read(&opaque_subject).unwrap().case().subject,
        pk(ctx.subject)
    );

    let mut records = [
        ctx.case,
        ctx.policy,
        ctx.subject,
        ctx.vault,
        ctx.author_receipt(),
    ]
    .into_iter()
    .zip(&original)
    .map(|(key, account)| observed_account(key, account, clock.slot))
    .collect::<Vec<_>>();
    for index in 0..records.len() {
        let original_record = records[index];
        records[index].slot -= 1;
        assert_eq!(
            CaseView::read(
                observation,
                ctx.case_address(),
                records[0],
                records[1],
                records[2],
                records[3],
                records[4],
            )
            .unwrap_err(),
            SdkError::InconsistentObservation
        );
        records[index] = original_record;
        records[index].key = pk(ctx.actor(Actor::Outsider));
        assert_eq!(
            CaseView::read(
                observation,
                ctx.case_address(),
                records[0],
                records[1],
                records[2],
                records[3],
                records[4],
            )
            .unwrap_err(),
            SdkError::InvalidRelationship
        );
        records[index] = original_record;
    }

    for index in [0, 1, 3, 4] {
        let mut corrupt = original.clone();
        corrupt[index].owner = ctx.actor(Actor::Outsider);
        assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidAccount);
    }
    for index in 0..5 {
        let mut corrupt = original.clone();
        corrupt[index].executable = true;
        assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidAccount);
    }
    let mut corrupt = original.clone();
    corrupt[0].data.truncate(8);
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidAccount);
    corrupt = original.clone();
    corrupt[1].data[0] ^= 1;
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidAccount);
    let mut policy = ctx.policy_state();
    policy.rules.audit_fee = 101;
    corrupt = original.clone();
    replace_state(&mut corrupt[1], &policy);
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidState);
    let mut case = ctx.case_state();
    case.contributor_entitlement = u128::MAX;
    case.contributor_unpaid = u128::MAX;
    corrupt = original.clone();
    replace_state(&mut corrupt[0], &case);
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidState);
    for unclaimed_contributors in [0, case.challenge_principal + 1] {
        let mut case = ctx.case_state();
        case.unclaimed_contributors = unclaimed_contributors;
        corrupt = original.clone();
        replace_state(&mut corrupt[0], &case);
        assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidState);
    }
    let mut case = ctx.case_state();
    case.contributor_unpaid = 0;
    corrupt = original.clone();
    replace_state(&mut corrupt[0], &case);
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InvalidState);
    // Retain a valid Token account but remove backing in the supplied observation.
    corrupt = original.clone();
    corrupt[3].data[64..72].copy_from_slice(&0u64.to_le_bytes());
    assert_eq!(read(&corrupt).unwrap_err(), SdkError::InsufficientBacking);

    let view = read(&original).unwrap();
    let receipt = ctx.account(ctx.receipt(Actor::Alice));
    let mut record = observed_account(ctx.receipt(Actor::Alice), &receipt, clock.slot);
    assert_eq!(
        view.contributor_claim(pk(ctx.actor(Actor::Bob)), record),
        Err(SdkError::InvalidRelationship)
    );
    record.slot -= 1;
    assert_eq!(
        view.contributor_claim(pk(ctx.actor(Actor::Alice)), record),
        Err(SdkError::InconsistentObservation)
    );
    let mut receipt_data = receipt;
    let mut receipt_state = ctx.receipt_state(Actor::Alice);
    // An oversized receipt exceeds all recorded deposits; an understated last
    // receipt would leave unpaid funds after its optimistic refund.
    for principal in [99, 19] {
        receipt_state.principal = principal;
        replace_state(&mut receipt_data, &receipt_state);
        assert_eq!(
            view.contributor_claim(
                pk(ctx.actor(Actor::Alice)),
                observed_account(ctx.receipt(Actor::Alice), &receipt_data, clock.slot)
            ),
            Err(SdkError::InvalidState)
        );
    }
}

#[test]
fn sdk_rejects_corrupt_committee_votes_and_unpaid_audited_fees() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    ctx.deliver(&ctx.audit_decision(AuditAnswer::Yes));
    let original = ctx.account(ctx.case);
    for variant in 0..4 {
        let mut case = ctx.case_state();
        match variant {
            0 => case.audit.votes.push(case.audit.votes[0]),
            1 => case.audit.votes[0].auditor_index = 1,
            2 => case.audit.votes.clear(),
            _ => case.fee_claimed = false,
        }
        let mut corrupt = original.clone();
        replace_state(&mut corrupt, &case);
        let clock: Clock = ctx.clock();
        let policy = ctx.account(ctx.policy);
        let subject = ctx.account(ctx.subject);
        let vault = ctx.account(ctx.vault);
        let author_receipt = ctx.account(ctx.author_receipt());
        assert_eq!(
            CaseView::read(
                Observation {
                    slot: clock.slot,
                    unix_timestamp: clock.unix_timestamp
                },
                ctx.case_address(),
                observed_account(ctx.case, &corrupt, clock.slot),
                observed_account(ctx.policy, &policy, clock.slot),
                observed_account(ctx.subject, &subject, clock.slot),
                observed_account(ctx.vault, &vault, clock.slot),
                observed_account(ctx.author_receipt(), &author_receipt, clock.slot),
            )
            .unwrap_err(),
            SdkError::InvalidState
        );
    }
}

#[test]
fn sdk_authenticates_author_receipt_role_principal_and_claim_progress() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 40);
    let clock: Clock = ctx.clock();
    let observation = Observation {
        slot: clock.slot,
        unix_timestamp: clock.unix_timestamp,
    };
    let records =
        [ctx.case, ctx.policy, ctx.subject, ctx.vault].map(|address| ctx.account(address));
    let original = ctx.account(ctx.author_receipt());
    let read = |address, author_receipt: &Account| {
        CaseView::read(
            observation,
            ctx.case_address(),
            observed_account(ctx.case, &records[0], clock.slot),
            observed_account(ctx.policy, &records[1], clock.slot),
            observed_account(ctx.subject, &records[2], clock.slot),
            observed_account(ctx.vault, &records[3], clock.slot),
            observed_account(address, author_receipt, clock.slot),
        )
    };
    for variant in 0..6 {
        let mut receipt = ctx.author_receipt_state();
        match variant {
            0 => receipt.side = 1,
            1 => receipt.principal -= 1,
            2 => receipt.claimed = true,
            3 => receipt.owner = pk(ctx.actor(Actor::Outsider)),
            4 => receipt.case = pk(ctx.subject),
            _ => receipt.bump ^= 1,
        }
        let mut corrupt = original.clone();
        replace_state(&mut corrupt, &receipt);
        let expected = if variant < 3 {
            SdkError::InvalidState
        } else {
            SdkError::InvalidRelationship
        };
        assert_eq!(read(ctx.author_receipt(), &corrupt).unwrap_err(), expected);
    }
    assert_eq!(
        read(
            ctx.receipt(Actor::Alice),
            &ctx.account(ctx.receipt(Actor::Alice))
        )
        .unwrap_err(),
        SdkError::InvalidRelationship
    );
    ctx.finalize_optimistic_at_funding_deadline();
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    assert_eq!(ctx.view().author_claim(), ClaimState::Claimed);
    assert_eq!(ctx.view().vault_balance(), 40);
    assert_eq!(
        contributor(&ctx, Actor::Alice),
        ClaimState::Unclaimed { amount: 40 }
    );
}
