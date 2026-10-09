use anchor_lang::{AccountSerialize, error::ErrorCode};
use sha2::{Digest, Sha256};
use tekmerion::{StakeReceipt, errors::TekmerionError as Error};

use crate::support::{
    Actor, BALANCE, BOND, ExpectedError, TestContext, pk, writable_account_meta_mut,
};

#[test]
fn author_stake_receipt_binds_actual_principal_and_authenticates_both_claim_paths() {
    let mut ctx = TestContext::new();
    ctx.create_case();
    ctx.contribute(Actor::Alice, 40);
    let receipt = ctx.author_receipt_state();
    assert_eq!(receipt.case, pk(ctx.case));
    assert_eq!(receipt.owner, pk(ctx.actor(Actor::Author)));
    assert_eq!(receipt.side, 0);
    assert_eq!(receipt.principal, BOND);
    assert!(receipt.principal > ctx.policy_state().rules.author_bond);
    assert!(!receipt.claimed);
    assert_eq!(
        ctx.account(ctx.author_receipt()).data.len(),
        StakeReceipt::SPACE
    );
    assert_eq!(ctx.receipt_state(Actor::Alice).side, 1);
    ctx.finalize_optimistic_at_funding_deadline();
    let author_claim = ctx.author_claim_ix();
    let contributor_claim = ctx.contributor_claim_ix(Actor::Alice);
    assert_eq!(author_claim.data, Sha256::digest(b"global:claim")[..8]);
    assert_eq!(author_claim.data, contributor_claim.data);
    let schema = |instruction: &solana_instruction::Instruction| {
        instruction
            .accounts
            .iter()
            .map(|account| (account.is_signer, account.is_writable))
            .collect::<Vec<_>>()
    };
    assert_eq!(author_claim.accounts.len(), 8);
    assert_eq!(schema(&author_claim), schema(&contributor_claim));
    let aliases: Vec<_> = author_claim
        .accounts
        .iter()
        .filter(|account| account.pubkey == ctx.author_receipt())
        .collect();
    assert_eq!(aliases.len(), 2);
    assert_eq!(
        aliases.iter().filter(|account| account.is_writable).count(),
        1
    );
    let original = ctx.account(ctx.author_receipt());
    for variant in 0..4 {
        let mut receipt = ctx.author_receipt_state();
        match variant {
            0 => receipt.side = 1,
            1 => receipt.principal = ctx.policy_state().rules.author_bond,
            2 => receipt.owner = pk(ctx.actor(Actor::Outsider)),
            _ => receipt.principal = 0,
        }
        let mut forged = original.clone();
        receipt
            .try_serialize(&mut forged.data.as_mut_slice())
            .unwrap();
        ctx.set_account(ctx.author_receipt(), forged);
        ctx.reject(
            &[ctx.author_claim_ix()],
            &[Actor::Author],
            ExpectedError::Protocol(Error::InvalidReceipt),
        );
        ctx.reject(
            &[ctx.contributor_claim_ix(Actor::Alice)],
            &[Actor::Alice],
            ExpectedError::Protocol(Error::InvalidReceipt),
        );
        ctx.set_account(ctx.author_receipt(), original.clone());
    }
    for actor in [Actor::Author, Actor::Alice] {
        let mut instruction = if matches!(actor, Actor::Author) {
            ctx.author_claim_ix()
        } else {
            ctx.contributor_claim_ix(actor)
        };
        // The backing author receipt remains a separate readonly role even when
        // the author is also the claiming receipt owner.
        let backing = instruction
            .accounts
            .iter_mut()
            .find(|account| account.pubkey == ctx.author_receipt() && !account.is_writable)
            .unwrap();
        backing.pubkey = ctx.receipt(Actor::Alice);
        ctx.reject(
            &[instruction],
            &[actor],
            ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
        );
    }
    let mut substituted = ctx.author_claim_ix();
    writable_account_meta_mut(&mut substituted, ctx.author_receipt()).pubkey =
        ctx.receipt(Actor::Alice);
    ctx.reject(
        &[substituted],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::ConstraintSeeds),
    );
    let mut readonly = ctx.author_claim_ix();
    writable_account_meta_mut(&mut readonly, ctx.author_receipt()).is_writable = false;
    ctx.reject(
        &[readonly],
        &[Actor::Author],
        ExpectedError::Anchor(ErrorCode::ConstraintMut),
    );
    let challenge_original = ctx.account(ctx.receipt(Actor::Alice));
    for variant in 0..4 {
        let mut challenge = ctx.receipt_state(Actor::Alice);
        match variant {
            0 => challenge.side = tekmerion::AUTHOR_SIDE,
            1 => challenge.principal = 0,
            2 => challenge.principal = ctx.case_state().challenge_principal + 1,
            _ => challenge.owner = pk(ctx.actor(Actor::Outsider)),
        }
        let mut forged = challenge_original.clone();
        challenge
            .try_serialize(&mut forged.data.as_mut_slice())
            .unwrap();
        ctx.set_account(ctx.receipt(Actor::Alice), forged);
        ctx.reject(
            &[ctx.contributor_claim_ix(Actor::Alice)],
            &[Actor::Alice],
            if variant == 3 {
                ExpectedError::Anchor(ErrorCode::ConstraintHasOne)
            } else {
                ExpectedError::Protocol(Error::InvalidReceipt)
            },
        );
        ctx.set_account(ctx.receipt(Actor::Alice), challenge_original.clone());
    }
    let immutable_case = ctx.account(ctx.case);
    ctx.ok(&[ctx.author_claim_ix()], &[Actor::Author]);
    assert!(ctx.author_receipt_state().claimed);
    assert_eq!(ctx.account(ctx.case), immutable_case);
    assert_eq!(ctx.unpaid(), 40);
    assert_eq!(ctx.view().vault_balance(), 40);
    ctx.ok(&[ctx.contributor_claim_ix(Actor::Alice)], &[Actor::Alice]);
    assert_eq!(ctx.unpaid(), 0);
    assert_eq!(ctx.amount(ctx.token(Actor::Author)), BALANCE);
    assert_eq!(ctx.amount(ctx.token(Actor::Alice)), BALANCE);
    ctx.reject(
        &[ctx.author_claim_ix()],
        &[Actor::Author],
        ExpectedError::Protocol(Error::AlreadyClaimed),
    );
}
