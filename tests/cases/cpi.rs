use anchor_lang::{InstructionData, ToAccountMetas, error::ErrorCode, prelude::Pubkey};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use tekmerion::{
    AuditAnswer, AuditReason, AuditVote, Case, CasePhase, CaseResult, CompletionMethod, Policy,
    errors::TekmerionError as Error,
};
use tekmerion_sdk::instructions::{self as sdk, CaseAddress, CreatePolicy};

use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, MINT, SYSTEM, TARGET, TOKEN, TestContext,
    account_meta_mut, committee_rules, pk, rules, setup,
};

fn address(key: Pubkey) -> Address {
    Address::new_from_array(key.to_bytes())
}

fn context() -> TestContext {
    let mut program = setup::programs::program_test();
    program.add_program("sdk_consumer", sdk_consumer::ID, None);
    TestContext::with_program_test(program, Actor::ClientAuthority, rules(), BALANCE)
}

fn authority(ctx: &TestContext) -> Pubkey {
    Pubkey::find_program_address(
        &[
            sdk_consumer::AUTHORITY_SEED,
            ctx.actor(Actor::ClientAuthority).as_ref(),
        ],
        &sdk_consumer::ID,
    )
    .0
}

fn create_case(ctx: &TestContext, case: CaseAddress, previous: Option<Pubkey>) -> Instruction {
    let mut accounts = sdk_consumer::accounts::CreateCase {
        payer: pk(ctx.actor(Actor::Payer)),
        controller: pk(ctx.actor(Actor::ClientAuthority)),
        author: pk(ctx.actor(Actor::Author)),
        authority: authority(ctx),
        policy: case.policy,
        subject: case.subject,
        case: case.address(),
        author_receipt: case.receipt(&pk(ctx.actor(Actor::Author))),
        mint: pk(MINT),
        source: pk(ctx.token(Actor::Author)),
        vault: case.vault(),
        token_program: pk(TOKEN),
        system_program: pk(SYSTEM),
        tekmerion_program: tekmerion::ID,
    }
    .to_account_metas(None);
    if let Some(previous) = previous {
        accounts.push(AccountMeta::new_readonly(previous, false));
    }
    Instruction {
        program_id: sdk_consumer::ID,
        accounts,
        data: sdk_consumer::instruction::CreateCase {
            nonce: case.nonce,
            author_principal: BOND,
            agreement_hash: [0xa1; 32],
            result_hash: [0x91; 32],
        }
        .data(),
    }
}

fn submit_audit(ctx: &TestContext, destinations: &[AccountMeta]) -> Instruction {
    let mut accounts = sdk_consumer::accounts::SubmitAudit {
        controller: pk(ctx.actor(Actor::ClientAuthority)),
        authority: authority(ctx),
        case: pk(ctx.case),
        policy: pk(ctx.policy),
        mint: pk(MINT),
        vault: pk(ctx.vault),
        token_program: pk(TOKEN),
        tekmerion_program: tekmerion::ID,
    }
    .to_account_metas(None);
    accounts.extend_from_slice(destinations);
    Instruction {
        program_id: sdk_consumer::ID,
        accounts,
        data: sdk_consumer::instruction::SubmitAudit {
            answer: AuditAnswer::No,
            reason: AuditReason::Decisive,
        }
        .data(),
    }
}

#[test]
fn cpi_creation_uses_a_controller_pda_and_forwards_successor_history() {
    let mut ctx = context();
    let client_authority = authority(&ctx);
    let policy = Policy::address(&pk(ctx.actor(Actor::Admin)), 1).0;
    ctx.ok(
        &[sdk::create_policy(CreatePolicy {
            payer: pk(ctx.actor(Actor::Payer)),
            admin: pk(ctx.actor(Actor::Admin)),
            client_authority,
            identifier: 1,
            rules: rules(),
        })],
        &[Actor::Admin],
    );
    ctx.policy = address(policy);
    ctx.authority = address(client_authority);
    ctx.case = address(ctx.case_address().address());
    ctx.vault = address(ctx.case_address().vault());
    ctx.protected.extend([
        ctx.policy,
        ctx.authority,
        ctx.case,
        ctx.vault,
        ctx.author_receipt(),
    ]);

    // A transaction cannot supply the program-derived signature itself.
    let mut unsigned = ctx.submit_case_ix(BOND);
    account_meta_mut(&mut unsigned, ctx.authority).is_signer = false;
    ctx.reject(
        &[unsigned],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );

    let create = create_case(&ctx, ctx.case_address(), None);
    assert!(
        !create
            .accounts
            .iter()
            .find(|account| account.pubkey == client_authority)
            .unwrap()
            .is_signer
    );
    let mut wrong_controller = create.clone();
    account_meta_mut(&mut wrong_controller, ctx.actor(Actor::ClientAuthority)).pubkey =
        ctx.actor(Actor::Outsider);
    ctx.reject(
        &[wrong_controller],
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );

    // The controller and author sign; the fixture's invoke_signed supplies the PDA.
    ctx.ok(&[create], &[Actor::Author, Actor::ClientAuthority]);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funding);
    assert_eq!(ctx.case_state().author_principal, BOND);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
    assert_eq!(ctx.amount(ctx.vault), BOND);
    assert_eq!(ctx.author_receipt_state().principal, BOND);
    assert!(!ctx.author_receipt_state().claimed);
    assert_eq!(ctx.policy_state().active_uses, 1);
    ctx.view();

    ctx.contribute(Actor::Alice, TARGET);
    ctx.set_time(ctx.case_state().audit.deadline.unwrap());
    ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
    assert_eq!(
        ctx.case_state().completion_method(),
        Some(CompletionMethod::TechnicalFailure)
    );
    let previous = ctx.account(ctx.case);
    let previous_vault = ctx.account(ctx.vault);
    let previous_receipt = ctx.account(ctx.author_receipt());
    let successor = CaseAddress {
        nonce: 1,
        ..ctx.case_address()
    };

    ctx.reject(
        &[create_case(&ctx, successor, None)],
        &[Actor::Author, Actor::ClientAuthority],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    for key in [
        successor.address(),
        successor.vault(),
        successor.receipt(&pk(ctx.actor(Actor::Author))),
    ] {
        assert!(ctx.get_account(&address(key)).is_none());
    }
    assert_eq!(ctx.policy_state().active_uses, 1);

    let create_successor = create_case(&ctx, successor, Some(pk(ctx.case)));
    assert_eq!(
        create_successor.accounts.last(),
        Some(&AccountMeta::new_readonly(ctx.case, false))
    );
    ctx.ok(
        &[create_successor],
        &[Actor::Author, Actor::ClientAuthority],
    );
    let state: Case = ctx.state(address(successor.address()));
    assert_eq!(state.previous_case, Some(pk(ctx.case)));
    assert_eq!(state.nonce, 1);
    assert_eq!(state.phase(), CasePhase::Funding);
    assert_eq!(ctx.policy_state().active_uses, 2);
    assert_eq!(ctx.amount(address(successor.vault())), BOND);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 2 * BOND);
    assert_eq!(ctx.account(ctx.case), previous);
    assert_eq!(ctx.account(ctx.vault), previous_vault);
    assert_eq!(ctx.account(ctx.author_receipt()), previous_receipt);
}

#[test]
fn cpi_decisive_vote_preserves_destination_order_privileges_and_atomic_payments() {
    let mut ctx = context();
    let mut committee = committee_rules(2, 3);
    committee.auditors[0].authority = authority(&ctx);
    ctx.ok(&[ctx.update_policy_ix(committee)], &[Actor::Admin]);
    ctx.create_case();
    ctx.contribute(Actor::Alice, TARGET);
    let decision = ctx.audit_decision(AuditAnswer::No);
    ctx.vote(Actor::Auditor2, &decision);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);

    let destinations = [
        AccountMeta::new(ctx.token(Actor::FeeRecipient), false),
        AccountMeta::new(ctx.token(Actor::FeeRecipient2), false),
    ];
    ctx.reject(
        &[submit_audit(&ctx, &[])],
        &[Actor::ClientAuthority],
        ExpectedError::Protocol(Error::InvalidPayoutAccounts),
    );
    ctx.reject(
        &[submit_audit(
            &ctx,
            &[destinations[1].clone(), destinations[0].clone()],
        )],
        &[Actor::ClientAuthority],
        ExpectedError::Protocol(Error::InvalidFeeRecipient),
    );
    let mut readonly = destinations.clone();
    readonly[1].is_writable = false;
    // The second payout fails after the first transfer; both CPIs must roll back.
    ctx.reject(
        &[submit_audit(&ctx, &readonly)],
        &[Actor::ClientAuthority],
        ExpectedError::Protocol(Error::InvalidPayoutAccounts),
    );
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    assert!(!ctx.case_state().fee_claimed);
    assert_eq!(ctx.amount(ctx.vault), BOND + TARGET);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE);

    let vote = submit_audit(&ctx, &destinations);
    ctx.ok(std::slice::from_ref(&vote), &[Actor::ClientAuthority]);
    let state = ctx.case_state();
    assert_eq!(state.phase(), CasePhase::Finalized);
    assert_eq!(state.result(), Some(CaseResult::No));
    assert_eq!(state.completion_method(), Some(CompletionMethod::Audited));
    assert_eq!(state.audit.response, Some(decision));
    assert_eq!(
        state.audit.votes,
        [
            AuditVote {
                auditor_index: 1,
                response: decision,
            },
            AuditVote {
                auditor_index: 0,
                response: decision,
            },
        ]
    );
    assert!(state.fee_claimed);
    assert_eq!(state.fee_entitlement, 7);
    assert_eq!(state.author_entitlement, 0);
    assert_eq!(state.contributor_entitlement, u128::from(BOND + TARGET - 7));
    assert_eq!(ctx.amount(ctx.vault), BOND + TARGET - 7);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 3);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient3)), BALANCE);
    ctx.view();

    ctx.reject(
        &[vote],
        &[Actor::ClientAuthority],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
}
