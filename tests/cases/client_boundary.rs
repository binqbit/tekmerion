use anchor_lang::{AccountSerialize, error::ErrorCode};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_message::Message;
use solana_transaction::Transaction;
use tekmerion::{
    AuditAnswer, AuditReason, Case, CaseResult, PolicyRules, errors::TekmerionError as Error,
};
use tekmerion_sdk::instructions::{
    CaseAddress, CreateCase, CreatePolicy, acquire_policy, create_case, create_policy, submit_audit,
};

use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, MAIN, SYSTEM, TestContext, account_meta_mut, pk, rules,
    writable_account_meta_mut,
};

fn acquire_policy_ix(ctx: &TestContext) -> Instruction {
    acquire_policy(pk(ctx.policy), pk(ctx.authority), pk(ctx.subject))
}

fn attempt_ix(ctx: &TestContext, case: CaseAddress) -> Instruction {
    create_case(
        case,
        CreateCase {
            payer: pk(ctx.actor(Actor::Payer)),
            client_authority: pk(ctx.authority),
            author: pk(ctx.actor(Actor::Author)),
            source: pk(ctx.token(Actor::Author)),
            author_principal: BOND,
            agreement_hash: [0xa1; 32],
            result_hash: [0x91; 32],
        },
    )
}

fn sign_creation(ctx: &TestContext, include_acquisition: bool) -> Transaction {
    let mut instructions = Vec::new();
    if include_acquisition {
        instructions.push(acquire_policy_ix(ctx));
    }
    instructions.push(ctx.submit_case_ix(BOND));

    let message = Message::new_with_blockhash(
        &instructions,
        Some(&ctx.actor(Actor::Payer)),
        &ctx.latest_blockhash(),
    );
    let transaction = Transaction::new(
        &[
            &ctx.keys[Actor::Payer as usize],
            &ctx.keys[Actor::ClientAuthority as usize],
            &ctx.keys[Actor::Author as usize],
        ],
        message,
        ctx.latest_blockhash(),
    );
    transaction.verify().unwrap();
    assert!(wincode::serialize(&transaction).unwrap().len() <= 1232);
    transaction
}

fn changed_rules(ctx: &TestContext) -> PolicyRules {
    let mut rules = ctx.policy_state().rules;
    rules.auditors[0].authority = pk(ctx.actor(Actor::Auditor2));
    rules.auditors[0].fee_recipient = pk(ctx.actor(Actor::FeeRecipient2));
    rules.audit_fee = 100;
    assert!(rules.has_valid_parameters());
    rules
}

#[test]
fn first_creation_uses_execution_time_rules_even_with_atomic_acquisition() {
    for include_acquisition in [false, true] {
        let mut ctx = TestContext::new();
        let reviewed = ctx.policy_state().rules;
        assert_eq!(ctx.policy_state().active_uses, 0);
        assert_eq!(reviewed.audit_fee, 7);
        assert_eq!(
            reviewed.auditors[0].authority,
            pk(ctx.actor(Actor::Auditor1))
        );
        let signed = sign_creation(&ctx, include_acquisition);
        let signature = signed.signatures[0];
        let original_wire = wincode::serialize(&signed).unwrap();

        // Admin is authorized to edit an unused Policy. A signature binds the
        // Policy address, not the rules read before that first use executes.
        let replacement = changed_rules(&ctx);
        ctx.ok(
            &[ctx.update_policy_ix(replacement.clone())],
            &[Actor::Admin],
        );
        assert_eq!(ctx.transaction_status(&signature), None);
        let original: Transaction = wincode::deserialize(&original_wire).unwrap();
        assert_eq!(original, signed);
        assert_eq!(wincode::serialize(&original).unwrap(), original_wire);
        let receipt = ctx.process_signed(original).unwrap();

        assert_eq!(receipt.signature, signature);
        assert_eq!(ctx.transaction_status(&signature), Some(Ok(())));
        assert_eq!(ctx.policy_state().rules, replacement);
        assert_eq!(
            ctx.policy_state().active_uses,
            1 + u64::from(include_acquisition)
        );
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - BOND);
        ctx.contribute(Actor::Alice, 100);
        let vote = submit_audit(
            ctx.case_address(),
            pk(ctx.actor(Actor::Auditor2)),
            AuditAnswer::Unknown,
            AuditReason::Inconclusive,
            &[pk(ctx.token(Actor::FeeRecipient2))],
        );
        ctx.ok(&[vote], &[Actor::Auditor2]);

        assert_eq!(ctx.case_state().result(), Some(CaseResult::Unknown));
        assert_eq!(ctx.case_state().author_entitlement, BOND - 100);
        assert_eq!(BOND - reviewed.audit_fee, 118);
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 100);
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
        ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
        ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 100);
        assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
        assert_eq!(ctx.view().vault_balance(), 0);
    }
}

#[test]
fn confirmed_acquisition_before_review_and_signature_preserves_agreed_rules() {
    let mut ctx = TestContext::new();
    let acquisition = ctx.ok(&[acquire_policy_ix(&ctx)], &[Actor::ClientAuthority]);
    // This is confirmation in the local bank, not a public-chain finality claim.
    assert_eq!(ctx.transaction_status(&acquisition.signature), Some(Ok(())));
    assert_eq!(ctx.policy_state().active_uses, 1);
    let reviewed = ctx.policy_state().rules;
    let signed = sign_creation(&ctx, false);
    let original_wire = wincode::serialize(&signed).unwrap();

    ctx.reject(
        &[ctx.update_policy_ix(changed_rules(&ctx))],
        &[Actor::Admin],
        ExpectedError::Protocol(Error::PolicyInUse),
    );
    let original: Transaction = wincode::deserialize(&original_wire).unwrap();
    assert_eq!(original, signed);
    assert_eq!(wincode::serialize(&original).unwrap(), original_wire);
    ctx.process_signed(original).unwrap();
    ctx.contribute(Actor::Alice, 100);
    ctx.deliver(&ctx.audit_decision(AuditAnswer::Unknown));

    assert_eq!(ctx.policy_state().rules, reviewed);
    assert_eq!(ctx.policy_state().active_uses, 2);
    assert_eq!(
        ctx.case_state().author_entitlement,
        BOND - reviewed.audit_fee
    );
    assert_eq!(ctx.case_state().author_entitlement, 118);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 7);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE);
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 7);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
    assert_eq!(ctx.view().vault_balance(), 0);
}

#[test]
fn policy_acquisition_authenticates_only_the_configured_signer_and_retains_its_use() {
    let mut default_ctx = TestContext::new();
    let mut unsigned = acquire_policy_ix(&default_ctx);
    account_meta_mut(&mut unsigned, default_ctx.authority).is_signer = false;
    default_ctx.reject(
        &[unsigned],
        &[],
        ExpectedError::Anchor(ErrorCode::AccountNotSigner),
    );
    let mut substituted = acquire_policy_ix(&default_ctx);
    account_meta_mut(&mut substituted, default_ctx.authority).pubkey =
        default_ctx.actor(Actor::Admin);
    default_ctx.reject(
        &[substituted],
        &[Actor::Admin],
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );
    assert_eq!(default_ctx.policy_state().active_uses, 0);

    let mut ctx = TestContext::with_wallet_authority(Actor::Outsider);
    let acquire = acquire_policy_ix(&ctx);
    let mut executable = acquire.clone();
    account_meta_mut(&mut executable, ctx.subject).pubkey = MAIN;
    ctx.reject(
        &[executable],
        &[Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidSubject),
    );
    let mut extra = acquire.clone();
    extra
        .accounts
        .push(AccountMeta::new_readonly(ctx.actor(Actor::Bob), false));
    ctx.reject(
        &[extra],
        &[Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidSubject),
    );
    ctx.reject_at(
        &[acquire.clone(), ctx.update_policy_ix(rules())],
        &[Actor::Outsider, Actor::Admin],
        1,
        ExpectedError::Protocol(Error::PolicyInUse),
    );
    assert_eq!(ctx.policy_state().active_uses, 0);
    let subject_before = ctx.account(ctx.subject);
    ctx.ok(&[acquire], &[Actor::Outsider]);
    assert_eq!(ctx.policy_state().active_uses, 1);
    assert_eq!(ctx.account(ctx.subject), subject_before);
    ctx.reject(
        &[ctx.update_policy_ix(rules())],
        &[Actor::Admin],
        ExpectedError::Protocol(Error::PolicyInUse),
    );
    ctx.ok(
        &[ctx.submit_case_ix(BOND)],
        &[Actor::Author, Actor::Outsider],
    );
    assert_eq!(ctx.policy_state().active_uses, 2);
    assert_eq!(ctx.amount(ctx.vault), BOND);
    assert_eq!(ctx.account(ctx.subject), subject_before);
}

#[test]
fn tkt_077_088_successors_cannot_replace_pending_or_yes_cases() {
    let mut ctx = TestContext::with_wallet_authority(Actor::Outsider);
    ctx.ok(
        &[ctx.submit_case_ix(BOND)],
        &[Actor::Author, Actor::Outsider],
    );
    let next = CaseAddress {
        nonce: 1,
        ..ctx.case_address()
    };
    let successor = attempt_ix(&ctx, next);
    ctx.reject(
        std::slice::from_ref(&successor),
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    ctx.contribute(Actor::Alice, 100);
    ctx.reject(
        std::slice::from_ref(&successor),
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    ctx.deliver(&ctx.audit_decision(AuditAnswer::Yes));
    ctx.reject(
        std::slice::from_ref(&successor),
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    ctx.reject(
        &[successor],
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
    assert_eq!(ctx.policy_state().active_uses, 1);

    let mut optimistic = TestContext::with_wallet_authority(Actor::Outsider);
    optimistic.ok(
        &[optimistic.submit_case_ix(BOND)],
        &[Actor::Author, Actor::Outsider],
    );
    optimistic.finalize_optimistic_at_funding_deadline();
    let next = CaseAddress {
        nonce: 1,
        ..optimistic.case_address()
    };
    optimistic.reject(
        &[attempt_ix(&optimistic, next)],
        &[Actor::Author, Actor::Outsider],
        ExpectedError::Protocol(Error::InvalidHistory),
    );
}

#[test]
fn tkt_077_088_successors_bind_protocol_history_and_pay_fresh_collateral() {
    for answer in [Some(AuditAnswer::No), Some(AuditAnswer::Unknown), None] {
        let mut ctx = TestContext::with_wallet_authority(Actor::Outsider);
        ctx.ok(
            &[ctx.submit_case_ix(BOND)],
            &[Actor::Author, Actor::Outsider],
        );
        ctx.contribute(Actor::Alice, 100);
        if let Some(answer) = answer {
            ctx.deliver(&ctx.audit_decision(answer));
        } else {
            ctx.set_time(ctx.case_state().audit.deadline.unwrap());
            ctx.ok(&[ctx.finalize_timeout_ix()], &[]);
        }
        let next = CaseAddress {
            nonce: 1,
            ..ctx.case_address()
        };
        if answer == Some(AuditAnswer::No) {
            let foreign_policy = tekmerion::Policy::address(&pk(ctx.actor(Actor::Admin)), 1).0;
            ctx.ok(
                &[create_policy(CreatePolicy {
                    payer: pk(ctx.actor(Actor::Payer)),
                    admin: pk(ctx.actor(Actor::Admin)),
                    client_authority: pk(ctx.authority),
                    identifier: 1,
                    rules: rules(),
                })],
                &[Actor::Admin],
            );
            let mut missing_previous = attempt_ix(&ctx, next);
            assert_eq!(
                missing_previous.accounts.pop(),
                Some(AccountMeta::new_readonly(ctx.case, false))
            );
            let mut invalid_attempts = vec![missing_previous];
            for identity in [
                CaseAddress { nonce: 2, ..next },
                CaseAddress {
                    policy: foreign_policy,
                    ..next
                },
                CaseAddress {
                    subject: pk(ctx.token(Actor::Bob)),
                    ..next
                },
            ] {
                let mut wrong_previous = attempt_ix(&ctx, identity);
                // Deliberately replace the SDK's canonical predecessor with a
                // genuine terminal Case from a different protocol history.
                wrong_previous.accounts.last_mut().unwrap().pubkey = ctx.case;
                invalid_attempts.push(wrong_previous);
            }
            let mut first_with_previous = attempt_ix(
                &ctx,
                CaseAddress {
                    subject: pk(ctx.token(Actor::Bob)),
                    nonce: 0,
                    ..next
                },
            );
            first_with_previous
                .accounts
                .push(AccountMeta::new_readonly(ctx.case, false));
            invalid_attempts.push(first_with_previous);
            for invalid in invalid_attempts {
                ctx.reject(
                    &[invalid],
                    &[Actor::Author, Actor::Outsider],
                    ExpectedError::Protocol(Error::InvalidHistory),
                );
            }
            let original = ctx.account(ctx.case);
            let mut forged = original.clone();
            forged.owner = SYSTEM;
            ctx.set_account(ctx.case, forged);
            ctx.reject(
                &[attempt_ix(&ctx, next)],
                &[Actor::Author, Actor::Outsider],
                ExpectedError::Protocol(Error::InvalidHistory),
            );
            ctx.set_account(ctx.case, original);
        }
        let previous = ctx.account(ctx.case);
        let previous_vault = ctx.account(ctx.vault);
        let previous_author_receipt = ctx.account(ctx.author_receipt());
        let subject = ctx.account(ctx.subject);
        let successor = attempt_ix(&ctx, next);
        assert_eq!(
            successor.accounts.last(),
            Some(&AccountMeta::new_readonly(ctx.case, false))
        );
        ctx.ok(&[successor], &[Actor::Author, Actor::Outsider]);
        let next_key = Address::new_from_array(next.address().to_bytes());
        let next_vault = Address::new_from_array(next.vault().to_bytes());
        let next_author_receipt =
            Address::new_from_array(next.receipt(&pk(ctx.actor(Actor::Author))).to_bytes());
        let new_receipt: tekmerion::StakeReceipt = ctx.state(next_author_receipt);
        assert_eq!(new_receipt.side, 0);
        assert_eq!(new_receipt.principal, BOND);
        assert!(!new_receipt.claimed);
        let created: Case = ctx.state(next_key);
        assert_eq!(created.previous_case, Some(pk(ctx.case)));
        assert_eq!(created.nonce, 1);
        assert_eq!(created.subject, pk(ctx.subject));
        assert_eq!(created.policy, pk(ctx.policy));
        assert_eq!(created.author_principal, BOND);
        assert_eq!(ctx.amount(next_vault), BOND);
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE - 2 * BOND);
        assert_eq!(ctx.policy_state().active_uses, 2);
        assert_eq!(ctx.account(ctx.case), previous);
        assert_eq!(ctx.account(ctx.vault), previous_vault);
        assert_eq!(ctx.account(ctx.author_receipt()), previous_author_receipt);
        assert_eq!(ctx.account(ctx.subject), subject);

        // A later Case's independent receipt cannot authorize the previous author's claim.
        let mut foreign_author_receipt = ctx.author_claim_ix();
        writable_account_meta_mut(&mut foreign_author_receipt, ctx.author_receipt()).pubkey =
            next_author_receipt;
        ctx.reject(
            &[foreign_author_receipt],
            &[Actor::Author],
            ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
        );
        let next_receipt_before = ctx.account(next_author_receipt);
        ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
        assert!(ctx.author_receipt_state().claimed);
        assert_eq!(ctx.account(next_author_receipt), next_receipt_before);
        // Creating another Case cannot consume the predecessor's unpaid claims.
        let payout = u64::try_from(ctx.case_state().contributor_unpaid).unwrap();
        let before = ctx.amount(ctx.token(Actor::Alice));
        ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
        assert_eq!(ctx.amount(ctx.token(Actor::Alice)), before + payout);
        assert_eq!(ctx.amount(next_vault), BOND);
    }
}

#[test]
fn malformed_canonical_predecessors_cannot_authorize_or_charge_for_a_successor() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    ctx.deliver(&ctx.audit_decision(AuditAnswer::No));
    let next = CaseAddress {
        nonce: 1,
        ..ctx.case_address()
    };
    let successor = attempt_ix(&ctx, next);
    let next_key = Address::new_from_array(next.address().to_bytes());
    let next_vault = Address::new_from_array(next.vault().to_bytes());
    let next_receipt =
        Address::new_from_array(next.receipt(&pk(ctx.actor(Actor::Author))).to_bytes());
    let original = ctx.account(ctx.case);
    let previous_vault = ctx.account(ctx.vault);
    let author_balance = ctx.amount(ctx.token(Actor::Author));

    // Synthetic corruption at the genuine PDA exercises validation of retained
    // history, not just rejection of a different account address. Callers cannot
    // directly rewrite program-owned Case data with an ordinary transaction.
    for variant in 0..11 {
        let mut forged = original.clone();
        match variant {
            0 => {
                forged.data.pop();
            }
            1 => forged.data.push(0),
            2 => forged.data[..8].fill(0),
            _ => {
                let mut previous = ctx.case_state();
                match variant {
                    3 => previous.bump ^= 1,
                    4 => previous.policy = pk(ctx.actor(Actor::Outsider)),
                    5 => previous.subject = pk(ctx.actor(Actor::Bob)),
                    6 => previous.nonce = 7,
                    7 => previous.finalized_at = None,
                    8 => previous.audit.response = None,
                    9 => previous.author = pk(ctx.actor(Actor::Bob)),
                    _ => previous.audit.deadline = previous.finalized_at,
                }
                previous
                    .try_serialize(&mut forged.data.as_mut_slice())
                    .unwrap();
            }
        }
        ctx.set_account(ctx.case, forged);
        ctx.reject(
            std::slice::from_ref(&successor),
            &ctx.creation_signers(),
            ExpectedError::Protocol(Error::InvalidHistory),
        );
        assert!(ctx.get_account(&next_key).is_none());
        assert!(ctx.get_account(&next_vault).is_none());
        assert!(ctx.get_account(&next_receipt).is_none());
        assert_eq!(ctx.policy_state().active_uses, 1);
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), author_balance);
        assert_eq!(ctx.account(ctx.vault), previous_vault);
        ctx.set_account(ctx.case, original.clone());
    }

    ctx.ok(&[successor], &ctx.creation_signers());
    let created: Case = ctx.state(next_key);
    assert_eq!(created.previous_case, Some(pk(ctx.case)));
    assert_eq!(created.nonce, 1);
    assert_eq!(created.author_principal, BOND);
    assert_eq!(ctx.amount(next_vault), BOND);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), author_balance - BOND);
    assert_eq!(ctx.policy_state().active_uses, 2);
    assert_eq!(ctx.account(ctx.case), original);
    assert_eq!(ctx.account(ctx.vault), previous_vault);
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
    assert_eq!(ctx.amount(ctx.vault), 0);
    assert_eq!(ctx.amount(next_vault), BOND);
}

#[test]
fn executable_subject_creation_rolls_back_every_initialized_account_and_bond() {
    let mut ctx = TestContext::new();
    // The invoked Tekmerion address is forcibly readonly at the transaction
    // boundary. Use the real Token program to reach the executable-subject guard.
    let executable = crate::support::TOKEN;
    let executable_case = CaseAddress {
        subject: pk(executable),
        ..ctx.case_address()
    };
    let executable_before = ctx.account(executable);
    assert!(executable_before.executable);
    let instruction = attempt_ix(&ctx, executable_case);
    ctx.reject(
        &[instruction],
        &ctx.creation_signers(),
        ExpectedError::Protocol(Error::InvalidSubject),
    );
    for address in [
        executable_case.address(),
        executable_case.vault(),
        executable_case.receipt(&pk(ctx.actor(Actor::Author))),
    ] {
        assert!(
            ctx.get_account(&Address::new_from_array(address.to_bytes()))
                .is_none()
        );
    }
    assert_eq!(ctx.policy_state().active_uses, 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    assert_eq!(ctx.account(executable), executable_before);
    ctx.create_case();
    assert_eq!(ctx.amount(ctx.vault), BOND);
    assert_eq!(ctx.policy_state().active_uses, 1);
}

#[test]
fn invalid_policy_creation_returns_rent_and_leaves_the_address_reusable() {
    let mut ctx = TestContext::new();
    let identifier = 1;
    let policy = tekmerion::Policy::address(&pk(ctx.actor(Actor::Admin)), identifier).0;
    let policy_key = Address::new_from_array(policy.to_bytes());
    for variant in 0..7 {
        let mut candidate = rules();
        let expected = match variant {
            0 => {
                candidate.auditors.clear();
                Error::InvalidRules
            }
            1 => {
                candidate.threshold = 0;
                Error::InvalidRules
            }
            2 => {
                candidate.audit_fee = candidate.author_bond + 1;
                Error::InvalidRules
            }
            3 => {
                let mut extra = candidate.auditors[0].clone();
                extra.fee_recipient = pk(ctx.actor(Actor::FeeRecipient2));
                candidate.auditors.push(extra);
                candidate.threshold = 2;
                Error::InvalidRules
            }
            4 => {
                let mut extra = candidate.auditors[0].clone();
                extra.authority = pk(ctx.actor(Actor::Auditor2));
                candidate.auditors.push(extra);
                candidate.threshold = 2;
                Error::InvalidRules
            }
            5 => {
                candidate.funding_duration = u64::MAX;
                Error::ArithmeticOverflow
            }
            _ => {
                candidate.audit_timeout = u64::MAX;
                Error::ArithmeticOverflow
            }
        };
        ctx.reject(
            &[create_policy(CreatePolicy {
                payer: pk(ctx.actor(Actor::Payer)),
                admin: pk(ctx.actor(Actor::Admin)),
                client_authority: pk(ctx.authority),
                identifier,
                rules: candidate,
            })],
            &[Actor::Admin],
            ExpectedError::Protocol(expected),
        );
        assert!(ctx.get_account(&policy_key).is_none());
        assert_eq!(ctx.policy_state().active_uses, 0);
    }
    ctx.ok(
        &[create_policy(CreatePolicy {
            payer: pk(ctx.actor(Actor::Payer)),
            admin: pk(ctx.actor(Actor::Admin)),
            client_authority: pk(ctx.authority),
            identifier,
            rules: rules(),
        })],
        &[Actor::Admin],
    );
    let created: tekmerion::Policy = ctx.state(policy_key);
    assert_eq!(created.rules, rules());
    assert_eq!(created.active_uses, 0);
    assert_eq!(created.admin, pk(ctx.actor(Actor::Admin)));
}

#[test]
fn both_document_commitments_are_required_before_author_funds_move() {
    use anchor_lang::InstructionData;
    for (agreement_hash, result_hash) in [([0; 32], [1; 32]), ([1; 32], [0; 32])] {
        let mut ctx = TestContext::new();
        let mut instruction = ctx.submit_case_ix(BOND);
        instruction.data = tekmerion::instruction::CreateCase {
            nonce: 0,
            author_principal: BOND,
            agreement_hash,
            result_hash,
        }
        .data();
        ctx.reject(
            &[instruction],
            &[Actor::ClientAuthority, Actor::Author],
            ExpectedError::Protocol(Error::InvalidCommitment),
        );
        assert_eq!(ctx.policy_state().active_uses, 0);
        assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    }
}
