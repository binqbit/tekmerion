use anchor_lang::error::ErrorCode;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tekmerion::errors::TekmerionError as Error;

use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, MAIN, SYSTEM, TOKEN, TestContext, account_meta_mut, pk,
    rules,
};

#[test]
fn configured_wallet_and_author_signatures_are_independently_required() {
    let mut ctx = TestContext::new();
    let subject_before = ctx.account(ctx.subject);
    assert_eq!(subject_before.owner, SYSTEM);
    assert!(!subject_before.executable);
    let mut unsigned = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut unsigned, ctx.authority).is_signer = false;
    ctx.reject(
        &[unsigned],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );
    let mut wallet = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut wallet, ctx.authority).pubkey = ctx.actor(Actor::Outsider);
    ctx.reject(
        &[wallet],
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );
    let mut missing_author = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut missing_author, ctx.actor(Actor::Author)).is_signer = false;
    ctx.reject(
        &[missing_author],
        &[Actor::ClientAuthority],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );
    let mut foreign_source = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut foreign_source, ctx.token(Actor::Author)).pubkey = ctx.token(Actor::Bob);
    ctx.reject(
        &[foreign_source],
        &ctx.creation_signers(),
        ExpectedError::Anchor(ErrorCode::ConstraintTokenOwner),
    );
    ctx.create_case();
    assert_eq!(ctx.policy_state().active_uses, 1);
    assert_eq!(ctx.account(ctx.subject), subject_before);
    assert_eq!(ctx.case_state().subject, pk(ctx.subject));
    ctx.reject(
        &[ctx.submit_case_ix(BOND)],
        &ctx.creation_signers(),
        ExpectedError::Custom(0),
    );
    assert_eq!(ctx.policy_state().active_uses, 1);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
}

#[test]
fn tkt_008_019_configured_wallet_authorizes_an_opaque_client_subject() {
    let mut ctx = TestContext::with_wallet_authority(Actor::Outsider);
    assert_eq!(
        ctx.policy_state().client_authority,
        pk(ctx.actor(Actor::Outsider))
    );
    assert_eq!(ctx.account(ctx.policy).data.len(), tekmerion::Policy::SPACE);

    // A token account can also serve as an opaque identity without any subject schema.
    ctx.subject = ctx.token(Actor::Bob);
    ctx.case = crate::support::pda(
        &[
            b"case",
            ctx.policy.as_ref(),
            ctx.subject.as_ref(),
            &0_u64.to_le_bytes(),
        ],
        crate::support::MAIN,
    );
    ctx.vault = crate::support::pda(&[b"vault", ctx.case.as_ref()], crate::support::MAIN);
    ctx.protected.extend([ctx.case, ctx.vault]);
    let subject_before = ctx.account(ctx.subject);

    let mut unsigned = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut unsigned, ctx.authority).is_signer = false;
    ctx.reject(
        &[unsigned],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );
    let mut wrong_signer = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut wrong_signer, ctx.authority).pubkey = ctx.actor(Actor::Admin);
    ctx.reject(
        &[wrong_signer],
        &[Actor::Author, Actor::Admin],
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );

    ctx.ok(
        &[ctx.submit_case_ix(BOND)],
        &[Actor::Author, Actor::Outsider],
    );
    let case = ctx.case_state();
    assert_eq!(case.subject, pk(ctx.subject));
    assert_eq!(case.author, pk(ctx.actor(Actor::Author)));
    assert_eq!(case.previous_case, None);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
    assert_eq!(ctx.amount(ctx.vault), BOND);
    assert_eq!(ctx.policy_state().active_uses, 1);
    assert_eq!(ctx.account(ctx.subject), subject_before);
    ctx.finalize_optimistic_at_funding_deadline();
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    assert_eq!(ctx.amount(ctx.vault), 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    assert_eq!(ctx.account(ctx.subject), subject_before);
}

#[test]
fn tkt_013_018_054_foreign_custody_receipts_and_destinations_never_authorize_spending() {
    let mut ctx = TestContext::new();
    let mut foreign_admin = ctx.update_policy_ix(rules());
    account_meta_mut(&mut foreign_admin, ctx.actor(Actor::Admin)).pubkey =
        ctx.actor(Actor::Outsider);
    ctx.reject(
        &[foreign_admin],
        &[Actor::Outsider],
        ExpectedError::Anchor(ErrorCode::ConstraintHasOne),
    );
    ctx.ok(&[ctx.update_policy_ix(rules())], &[Actor::Admin]);
    ctx.create_case();
    let mut wrong_source = ctx.contribute_ix(Actor::Alice, 10);
    account_meta_mut(&mut wrong_source, ctx.token(Actor::Alice)).pubkey = ctx.token(Actor::Bob);
    ctx.reject(
        &[wrong_source],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintTokenOwner),
    );
    let mut wrong_vault = ctx.contribute_ix(Actor::Alice, 10);
    account_meta_mut(&mut wrong_vault, ctx.vault).pubkey = ctx.token(Actor::Bob);
    ctx.reject(
        &[wrong_vault],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    let mut wrong_program = ctx.contribute_ix(Actor::Alice, 10);
    account_meta_mut(&mut wrong_program, TOKEN).pubkey = MAIN;
    ctx.reject(
        &[wrong_program],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::InvalidProgramId),
    );
    let mint = Keypair::new_from_array([0x58; 32]);
    ctx.create_mint(&mint);
    let foreign_source = ctx.create_token_account(40, ctx.actor(Actor::Alice), mint.pubkey());
    let mut wrong_mint = ctx.contribute_ix(Actor::Alice, 10);
    account_meta_mut(&mut wrong_mint, ctx.token(Actor::Alice)).pubkey = foreign_source;
    ctx.reject(
        &[wrong_mint],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintTokenMint),
    );
    ctx.reject(
        &[ctx.contribute_ix(Actor::Author, 1)],
        &[Actor::Author],
        ExpectedError::Protocol(Error::UnsupportedRole),
    );
    ctx.reject(
        &[ctx.contribute_ix(Actor::Alice, 0)],
        &[Actor::Alice],
        ExpectedError::Protocol(Error::InvalidContribution),
    );
    ctx.contribute(Actor::Alice, 10);
    ctx.contribute(Actor::Bob, 20);
    let mut wrong_receipt = ctx.contribute_ix(Actor::Alice, 1);
    account_meta_mut(&mut wrong_receipt, ctx.receipt(Actor::Alice)).pubkey =
        ctx.receipt(Actor::Bob);
    ctx.reject(
        &[wrong_receipt],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    ctx.contribute(Actor::Alice, 1);
    ctx.finalize_optimistic_at_funding_deadline();
    for actor in [Actor::Author, Actor::Alice] {
        let mut wrong_destination = if matches!(actor, Actor::Author) {
            ctx.author_claim_ix()
        } else {
            ctx.contributor_claim_ix(actor)
        };
        account_meta_mut(&mut wrong_destination, ctx.token(actor)).pubkey =
            ctx.token(Actor::Outsider);
        ctx.reject(
            &[wrong_destination],
            &[actor],
            ExpectedError::Anchor(ErrorCode::ConstraintTokenOwner),
        );
    }
    let mut foreign_receipt = ctx.contributor_claim_ix(Actor::Alice);
    account_meta_mut(&mut foreign_receipt, ctx.receipt(Actor::Alice)).pubkey =
        ctx.receipt(Actor::Bob);
    ctx.reject(
        &[foreign_receipt],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    for actor in [Actor::Author, Actor::Alice] {
        let mut missing_signer = if matches!(actor, Actor::Author) {
            ctx.author_claim_ix()
        } else {
            ctx.contributor_claim_ix(actor)
        };
        account_meta_mut(&mut missing_signer, ctx.actor(actor)).is_signer = false;
        ctx.reject(
            &[missing_signer],
            &[],
            ExpectedError::Anchor(ErrorCode::AccountNotSigner),
        );
    }
    let mut foreign_author = ctx.author_claim_ix();
    account_meta_mut(&mut foreign_author, ctx.actor(Actor::Author)).pubkey =
        ctx.actor(Actor::Outsider);
    ctx.reject(
        &[foreign_author],
        &[Actor::Outsider],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    ctx.ok(
        &[
            ctx.contributor_claim_ix(Actor::Alice),
            ctx.contributor_claim_ix(Actor::Bob),
            ctx.author_claim_ix(),
        ],
        &[Actor::Author, Actor::Alice, Actor::Bob],
    );
    assert_eq!(ctx.amount(ctx.vault), 0);
}

#[test]
fn tkt_064_account_owner_and_writable_privileges_are_runtime_boundaries() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    let mut readonly = ctx.contribute_ix(Actor::Alice, 10);
    account_meta_mut(&mut readonly, ctx.case).is_writable = false;
    ctx.reject(
        &[readonly],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintMut),
    );
    // Explicit adversarial injection isolates the canonical owner check; valid setup is always lawful.
    let original = ctx.account(ctx.case);
    let mut forged = original.clone();
    forged.owner = Address::new_from_array([0xa1; 32]);
    ctx.set_account(ctx.case, forged);
    ctx.reject(
        &[ctx.contribute_ix(Actor::Alice, 10)],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::AccountOwnedByWrongProgram),
    );
    ctx.set_account(ctx.case, original);
    ctx.contribute(Actor::Alice, 10);
}

#[test]
fn tkt_018_057_identical_cases_cannot_borrow_each_others_collateral_or_receipts() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 10);
    ctx.finalize_optimistic_at_funding_deadline();
    let first = (ctx.subject, ctx.case, ctx.vault, ctx.receipt(Actor::Alice));
    let immutable_first = ctx.account(ctx.case);
    ctx.select_fresh_subject(1);
    ctx.protected.insert(first.3);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 10);
    ctx.finalize_optimistic_at_funding_deadline();
    let mut swapped_vault = ctx.author_claim_ix();
    account_meta_mut(&mut swapped_vault, ctx.vault).pubkey = first.2;
    ctx.reject(
        &[swapped_vault],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    let mut swapped_receipt = ctx.contributor_claim_ix(Actor::Alice);
    account_meta_mut(&mut swapped_receipt, ctx.receipt(Actor::Alice)).pubkey = first.3;
    ctx.reject(
        &[swapped_receipt],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    ctx.ok(
        &[
            ctx.contributor_claim_ix(Actor::Alice),
            ctx.author_claim_ix(),
        ],
        &[Actor::Author, Actor::Alice],
    );
    assert_eq!(ctx.amount(ctx.vault), 0);
    assert_eq!(ctx.amount(first.2), crate::support::BOND + 10);
    assert_eq!(ctx.account(first.1), immutable_first);
    assert_eq!(ctx.policy_state().active_uses, 2);
    (ctx.subject, ctx.case, ctx.vault) = (first.0, first.1, first.2);
    ctx.ok(
        &[
            ctx.author_claim_ix(),
            ctx.contributor_claim_ix(Actor::Alice),
        ],
        &[Actor::Author, Actor::Alice],
    );
    assert_eq!(ctx.amount(ctx.vault), 0);
    assert_eq!(
        ctx.amount(ctx.token(Actor::Author)),
        crate::support::BALANCE
    );
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), crate::support::BALANCE);
}
