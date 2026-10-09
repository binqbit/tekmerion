use crate::support::{
    Actor, BALANCE, ExpectedError, MINT, TOKEN, TestContext, account_index, account_meta_mut,
    committee_rules,
};
use anchor_lang::error::ErrorCode;
use solana_instruction::AccountMeta;
use solana_instruction_error::InstructionError;
use solana_keypair::Keypair;
use solana_signer::Signer;
use tekmerion::{AuditAnswer, CasePhase, errors::TekmerionError as Error};

#[test]
fn final_vote_and_every_payment_roll_back_on_invalid_destinations_and_late_failure() {
    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    ctx.vote(Actor::Auditor1, &decision);
    let valid = ctx.submit_audit_ix(Actor::Auditor2, &decision);
    assert_eq!(valid.accounts.len(), 8);
    for extra in [false, true] {
        let mut invalid = valid.clone();
        if extra {
            invalid
                .accounts
                .push(AccountMeta::new(ctx.token(Actor::FeeRecipient3), false));
        } else {
            invalid.accounts.pop();
        }
        ctx.reject(
            &[invalid],
            &[Actor::Auditor2],
            ExpectedError::Protocol(Error::InvalidPayoutAccounts),
        );
    }
    let mut swapped = valid.clone();
    let first = account_index(&swapped, ctx.token(Actor::FeeRecipient));
    let second = account_index(&swapped, ctx.token(Actor::FeeRecipient2));
    swapped.accounts.swap(first, second);
    ctx.reject(
        &[swapped],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidFeeRecipient),
    );
    let mut wrong = valid.clone();
    account_meta_mut(&mut wrong, ctx.token(Actor::FeeRecipient2)).pubkey =
        ctx.token(Actor::Outsider);
    ctx.reject(
        &[wrong],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidFeeRecipient),
    );
    let mut readonly = valid.clone();
    account_meta_mut(&mut readonly, ctx.token(Actor::FeeRecipient2)).is_writable = false;
    ctx.reject(
        &[readonly],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidPayoutAccounts),
    );
    let foreign_mint = Keypair::new_from_array([0x65; 32]);
    ctx.create_mint(&foreign_mint);
    let foreign_token =
        ctx.create_token_account(0x66, ctx.actor(Actor::FeeRecipient2), foreign_mint.pubkey());
    let mut wrong_mint = valid.clone();
    account_meta_mut(&mut wrong_mint, ctx.token(Actor::FeeRecipient2)).pubkey = foreign_token;
    ctx.reject(
        &[wrong_mint],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    let mut substituted_mint = valid.clone();
    account_meta_mut(&mut substituted_mint, MINT).pubkey = foreign_mint.pubkey();
    ctx.reject(
        &[substituted_mint],
        &[Actor::Auditor2],
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );
    let mut duplicate = valid.clone();
    account_meta_mut(&mut duplicate, ctx.token(Actor::FeeRecipient2)).pubkey =
        ctx.token(Actor::FeeRecipient);
    let mut wrong_program = valid.clone();
    account_meta_mut(&mut wrong_program, ctx.token(Actor::FeeRecipient2)).pubkey =
        ctx.actor(Actor::Outsider);
    let mut malformed_destinations = vec![
        (
            duplicate,
            ExpectedError::Protocol(Error::InvalidFeeRecipient),
        ),
        (
            wrong_program,
            ExpectedError::Anchor(ErrorCode::AccountOwnedByWrongProgram),
        ),
    ];
    // These are reachable substitutions: System Program can allocate Token-owned
    // accounts without initializing them. No bank state is synthetically changed.
    for (seed, space, error) in [
        (0x68, 165, InstructionError::UninitializedAccount),
        (0x69, 8, InstructionError::InvalidAccountData),
    ] {
        let account = Keypair::new_from_array([seed; 32]);
        let create = solana_system_interface::instruction::create_account(
            &ctx.actor(Actor::Payer),
            &account.pubkey(),
            ctx.account(ctx.token(Actor::FeeRecipient2)).lamports,
            space,
            &TOKEN,
        );
        ctx.send(&[create], &[], &[&account]).unwrap();
        ctx.protected.insert(account.pubkey());
        let mut invalid = valid.clone();
        account_meta_mut(&mut invalid, ctx.token(Actor::FeeRecipient2)).pubkey = account.pubkey();
        malformed_destinations.push((invalid, ExpectedError::Native(error)));
    }
    for (invalid, expected) in malformed_destinations {
        let failure = ctx.reject_at(&[invalid], &[Actor::Auditor2], 0, expected);
        assert!(
            failure
                .meta
                .logs
                .iter()
                .any(|line| line == &format!("Program {TOKEN} success")),
            "first payout must execute before the second destination rejects"
        );
        assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
        assert_eq!(ctx.case_state().audit.votes.len(), 1);
    }
    // Fail on the second wallet: the first transfer and the quorum vote must revert too.
    ctx.freeze(ctx.token(Actor::FeeRecipient2), true);
    ctx.reject(
        std::slice::from_ref(&valid),
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    ctx.freeze(ctx.token(Actor::FeeRecipient2), false);
    ctx.freeze(ctx.vault, true);
    ctx.reject(
        std::slice::from_ref(&valid),
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidTokenAccount),
    );
    ctx.freeze(ctx.vault, false);
    let late_failure = ctx.transfer_ix(
        ctx.token(Actor::Outsider),
        ctx.token(Actor::Bob),
        Actor::Outsider,
        BALANCE + 1,
    );
    ctx.reject_at(
        &[valid.clone(), late_failure],
        &[Actor::Auditor2, Actor::Outsider],
        1,
        ExpectedError::Custom(1),
    );
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE);
    ctx.ok(std::slice::from_ref(&valid), &[Actor::Auditor2]);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.votes.len(), 2);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 3);
    ctx.reject(
        &[valid],
        &[Actor::Auditor2],
        ExpectedError::Protocol(Error::InvalidPhase),
    );
}

#[test]
fn presigned_nonfinal_vote_rejects_atomically_when_intervening_vote_makes_it_decisive() {
    use solana_message::Message;
    use solana_transaction::Transaction;
    use solana_transaction_error::TransactionError;

    let mut ctx = TestContext::with_rules(committee_rules(2, 3), BALANCE);
    ctx.create_case();
    ctx.contribute(Actor::Alice, 100);
    let decision = ctx.audit_decision(AuditAnswer::Yes);
    let nonfinal = ctx.submit_audit_ix(Actor::Auditor2, &decision);
    assert_eq!(nonfinal.accounts.len(), 6);
    ctx.new_blockhash();
    let message = Message::new_with_blockhash(
        std::slice::from_ref(&nonfinal),
        Some(&ctx.actor(Actor::Payer)),
        &ctx.latest_blockhash(),
    );
    let signed = Transaction::new(
        &[
            &ctx.keys[Actor::Payer as usize],
            &ctx.keys[Actor::Auditor2 as usize],
        ],
        message,
        ctx.latest_blockhash(),
    );
    signed.verify().unwrap();
    let signature = signed.signatures[0];
    let wire = wincode::serialize(&signed).unwrap();
    assert!(wire.len() <= 1232);

    ctx.vote(Actor::Auditor1, &decision);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    let original: Transaction = wincode::deserialize(&wire).unwrap();
    assert_eq!(original, signed);
    assert_eq!(ctx.transaction_status(&signature), None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let quoted_fee = runtime
        .block_on(
            ctx.context
                .banks_client
                .get_fee_for_message(original.message.clone()),
        )
        .unwrap()
        .expect("original blockhash must still be valid");
    assert!(quoted_fee > 0);
    let mut protected = ctx.protected.clone();
    protected.extend(original.message.account_keys.iter().copied());
    protected.remove(&ctx.actor(Actor::Payer));
    let before: Vec<_> = protected
        .iter()
        .map(|address| (*address, ctx.get_account(address)))
        .collect();
    let payer_before = ctx.account(ctx.actor(Actor::Payer));

    // Submit exactly the previously signed bytes; send() would rebuild and resign.
    let failure = ctx.process_signed(original).unwrap_err();
    let expected = TransactionError::InstructionError(
        0,
        InstructionError::Custom(Error::InvalidPayoutAccounts as u32 + 6000),
    );
    assert_eq!(failure.err, expected);
    assert_eq!(failure.meta.signature, signature);
    assert_eq!(failure.meta.fee, quoted_fee);
    assert_eq!(ctx.transaction_status(&signature), Some(Err(expected)));
    for (address, account) in before {
        assert_eq!(
            ctx.get_account(&address),
            account,
            "stale quorum transaction changed account {address}"
        );
    }
    let mut payer_after = ctx.account(ctx.actor(Actor::Payer));
    assert_eq!(payer_before.lamports - payer_after.lamports, quoted_fee);
    payer_after.lamports = payer_before.lamports;
    assert_eq!(payer_after, payer_before);
    assert_eq!(ctx.case_state().phase(), CasePhase::Funded);
    assert_eq!(ctx.case_state().audit.votes.len(), 1);
    assert!(!ctx.case_state().fee_claimed);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE);

    let decisive = ctx.submit_audit_ix(Actor::Auditor2, &decision);
    assert_eq!(decisive.accounts.len(), 8);
    assert_eq!(decisive.data, nonfinal.data);
    ctx.ok(&[decisive], &[Actor::Auditor2]);
    assert_eq!(ctx.case_state().phase(), CasePhase::Finalized);
    assert_eq!(ctx.case_state().audit.votes.len(), 2);
    assert_eq!(ctx.case_state().audit.response, Some(decision));
    assert!(ctx.case_state().fee_claimed);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient)), BALANCE + 4);
    assert_eq!(ctx.amount(ctx.token(Actor::FeeRecipient2)), BALANCE + 3);
}
