use anchor_lang::error::ErrorCode;
use solana_address::Address;
use solana_instruction::Instruction;
use tekmerion::{AuditAnswer, AuditReason, Case, CasePhase, CaseResult, errors::TekmerionError};
use tekmerion_sdk::instructions::{self as sdk, CaseAddress, ClaimDestination, Contribution};

use crate::support::{Actor, BALANCE, BOND, ExpectedError, TestContext, pk};

fn addr(key: anchor_lang::prelude::Pubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

fn identity(ctx: &TestContext, actor: Actor, nonce: u64) -> CaseAddress {
    CaseAddress {
        author: pk(ctx.actor(actor)),
        nonce,
        ..ctx.case_address()
    }
}

fn create(ctx: &TestContext, actor: Actor, nonce: u64) -> Instruction {
    sdk::create_case(
        identity(ctx, actor, nonce),
        sdk::CreateCase {
            payer: pk(ctx.actor(Actor::Payer)),
            client_authority: pk(ctx.authority),
            author: pk(ctx.actor(actor)),
            source: pk(ctx.token(actor)),
            funding: tekmerion::CaseFunding::Bonded { principal: BOND },
            agreement_hash: [0xa1; 32],
            result_hash: [0x91; 32],
        },
    )
}

fn contribute(ctx: &TestContext, case: CaseAddress, actor: Actor) -> Instruction {
    sdk::contribute(
        case,
        Contribution {
            payer: pk(ctx.actor(Actor::Payer)),
            owner: pk(ctx.actor(actor)),
            source: pk(ctx.token(actor)),
        },
        100,
    )
}

#[test]
fn authors_can_audit_in_parallel_and_claim_only_their_own_vaults() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    let first = ctx.case_address();
    let second = identity(&ctx, Actor::Alice, 0);
    assert_ne!(first.address(), second.address());
    ctx.ok(
        &[create(&ctx, Actor::Alice, 0)],
        &[Actor::Alice, Actor::ClientAuthority],
    );
    ctx.contribute(Actor::Bob, 100);
    ctx.ok(
        &[contribute(&ctx, second, Actor::Outsider)],
        &[Actor::Outsider],
    );
    assert_eq!(
        ctx.state::<Case>(addr(first.address())).phase(),
        CasePhase::Funded
    );
    assert_eq!(
        ctx.state::<Case>(addr(second.address())).phase(),
        CasePhase::Funded
    );
    assert_eq!(ctx.policy_state().active_uses, 2);

    // One author cannot create another pending attempt, even while a peer audits.
    ctx.reject(
        &[create(&ctx, Actor::Author, 1)],
        &[Actor::Author, Actor::ClientAuthority],
        ExpectedError::Protocol(TekmerionError::InvalidHistory),
    );
    let first_before = ctx.account(addr(first.address()));
    let first_vault_before = ctx.account(addr(first.vault()));
    ctx.ok(
        &[sdk::submit_audit(
            second,
            pk(ctx.actor(Actor::Auditor1)),
            AuditAnswer::Yes,
            AuditReason::Decisive,
            &[pk(ctx.token(Actor::FeeRecipient))],
        )],
        &[Actor::Auditor1],
    );
    assert_eq!(ctx.account(addr(first.address())), first_before);
    assert_eq!(ctx.account(addr(first.vault())), first_vault_before);
    assert_eq!(
        ctx.state::<Case>(addr(second.address())).result(),
        Some(CaseResult::Yes)
    );

    let claim = sdk::claim(
        second,
        second.author,
        ClaimDestination {
            owner: second.author,
            destination: pk(ctx.token(Actor::Alice)),
        },
    );
    let mut substituted = claim.clone();
    substituted
        .accounts
        .iter_mut()
        .find(|meta| meta.pubkey == addr(second.vault()))
        .unwrap()
        .pubkey = addr(first.vault());
    ctx.reject(
        &[substituted],
        &[Actor::Alice],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    ctx.ok(&[claim], &[Actor::Alice]);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE + 100 - 7);
    assert_eq!(ctx.amount(addr(second.vault())), 0);
    assert_eq!(ctx.account(addr(first.vault())), first_vault_before);

    // The other Case still completes normally after its peer has accepted Yes.
    ctx.deliver(&ctx.audit_decision(AuditAnswer::No));
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Bob)], &[Actor::Bob]);
    assert_eq!(ctx.amount(ctx.token(Actor::Bob)), BALANCE + BOND - 7);
    assert_eq!(ctx.amount(addr(first.vault())), 0);
    ctx.ok(
        &[create(&ctx, Actor::Author, 1)],
        &[Actor::Author, Actor::ClientAuthority],
    );
    let retry: Case = ctx.state(addr(identity(&ctx, Actor::Author, 1).address()));
    assert_eq!(retry.previous_case, Some(first.address()));
    assert_eq!(retry.author, first.author);
    ctx.reject(
        &[create(&ctx, Actor::Alice, 1)],
        &[Actor::Alice, Actor::ClientAuthority],
        ExpectedError::Protocol(TekmerionError::InvalidHistory),
    );
}

#[test]
fn another_authors_terminal_case_cannot_unlock_or_skip_a_predecessor() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Bob, 100);
    ctx.deliver(&ctx.audit_decision(AuditAnswer::No));
    let first = ctx.case_address();
    let terminal_before = ctx.account(addr(first.address()));
    let mut cross_author = create(&ctx, Actor::Alice, 1);
    cross_author.accounts.last_mut().unwrap().pubkey = addr(first.address());
    ctx.reject(
        &[cross_author],
        &[Actor::Alice, Actor::ClientAuthority],
        ExpectedError::Protocol(TekmerionError::InvalidHistory),
    );
    assert!(
        ctx.get_account(&addr(identity(&ctx, Actor::Alice, 1).address()))
            .is_none()
    );
    assert_eq!(ctx.account(addr(first.address())), terminal_before);
    ctx.ok(
        &[create(&ctx, Actor::Alice, 0)],
        &[Actor::Alice, Actor::ClientAuthority],
    );
    let second: Case = ctx.state(addr(identity(&ctx, Actor::Alice, 0).address()));
    assert_eq!(second.nonce, 0);
    assert!(second.previous_case.is_none());
    // A terminal peer also cannot replace this author's still-open nonce zero.
    let mut cross_author = create(&ctx, Actor::Alice, 1);
    cross_author.accounts.last_mut().unwrap().pubkey = addr(first.address());
    ctx.reject(
        &[cross_author],
        &[Actor::Alice, Actor::ClientAuthority],
        ExpectedError::Protocol(TekmerionError::InvalidHistory),
    );
    assert_eq!(ctx.policy_state().active_uses, 2);
}
