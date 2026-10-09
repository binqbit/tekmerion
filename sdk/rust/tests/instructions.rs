use anchor_lang::{prelude::Pubkey, system_program};
use solana_instruction::AccountMeta;
use tekmerion_sdk::{
    instructions::{CaseAddress, ClaimDestination, Contribution, claim, contribute, submit_audit},
    program::{self, AuditAnswer, AuditReason},
};

#[test]
fn claim_uses_one_instruction_and_canonical_receipts_for_both_roles() {
    let author = Pubkey::new_from_array([41; 32]);
    let contributor = Pubkey::new_from_array([42; 32]);
    let destination = Pubkey::new_from_array([43; 32]);
    let case = CaseAddress {
        policy: Pubkey::new_from_array([44; 32]),
        subject: Pubkey::new_from_array([45; 32]),
        nonce: 0,
    };

    for owner in [author, contributor] {
        let instruction = claim(case, author, ClaimDestination { owner, destination });

        assert_eq!(instruction.program_id, program::ID);
        // The public claim instruction has no role argument. Its account roles
        // and privileges are the same for either beneficiary.
        assert_eq!(instruction.data, [62, 198, 214, 193, 213, 159, 108, 210]);
        assert_eq!(
            instruction.accounts,
            [
                AccountMeta::new_readonly(owner, true),
                AccountMeta::new(case.address(), false),
                AccountMeta::new_readonly(case.receipt(&author), false),
                AccountMeta::new(case.receipt(&owner), false),
                AccountMeta::new_readonly(program::USDC_MINT, false),
                AccountMeta::new(case.vault(), false),
                AccountMeta::new(destination, false),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
            ]
        );

        let receipt_accounts: Vec<_> = instruction
            .accounts
            .iter()
            .filter(|account| account.pubkey == case.receipt(&owner))
            .collect();

        assert_eq!(receipt_accounts.len(), if owner == author { 2 } else { 1 });
        assert_eq!(
            receipt_accounts
                .iter()
                .filter(|account| account.is_writable)
                .count(),
            1
        );
    }
}

#[test]
fn audit_preserves_the_decision_and_caller_order_of_quorum_destinations() {
    let auditor = Pubkey::new_from_array([21; 32]);
    let case = CaseAddress {
        policy: Pubkey::new_from_array([22; 32]),
        subject: Pubkey::new_from_array([23; 32]),
        nonce: 2,
    };
    let first = Pubkey::new_from_array([28; 32]);
    let second = Pubkey::new_from_array([29; 32]);

    for (answer, reason, encoded) in [
        (AuditAnswer::Yes, AuditReason::Decisive, [0, 0]),
        (AuditAnswer::No, AuditReason::Decisive, [1, 0]),
        (AuditAnswer::Unknown, AuditReason::MissingEvidence, [2, 1]),
        (AuditAnswer::Unknown, AuditReason::StaleEvidence, [2, 2]),
    ] {
        for destinations in [vec![], vec![first, second], vec![second, first]] {
            let instruction = submit_audit(case, auditor, answer, reason, &destinations);
            let mut data = vec![129, 202, 59, 247, 177, 24, 17, 164];
            data.extend_from_slice(&encoded);

            assert_eq!(instruction.program_id, program::ID);
            assert_eq!(instruction.data, data);
            assert_eq!(
                instruction.accounts[..6],
                [
                    AccountMeta::new_readonly(auditor, true),
                    AccountMeta::new(case.address(), false),
                    AccountMeta::new_readonly(case.policy, false),
                    AccountMeta::new_readonly(program::USDC_MINT, false),
                    AccountMeta::new(case.vault(), false),
                    AccountMeta::new_readonly(anchor_spl::token::ID, false),
                ]
            );
            assert_eq!(instruction.accounts.len(), 6 + destinations.len());
            for (account, destination) in instruction.accounts[6..].iter().zip(destinations) {
                assert_eq!(*account, AccountMeta::new(destination, false));
            }
        }
    }
}

#[test]
fn contribution_keeps_the_requested_amount_and_separate_payer_and_token_owner() {
    let payer = Pubkey::new_from_array([1; 32]);
    let owner = Pubkey::new_from_array([2; 32]);
    let source = Pubkey::new_from_array([3; 32]);
    let case = CaseAddress {
        policy: Pubkey::new_from_array([5; 32]),
        subject: Pubkey::new_from_array([6; 32]),
        nonce: 0,
    };

    for requested in [1_u64, u64::MAX] {
        let instruction = contribute(
            case,
            Contribution {
                payer,
                owner,
                source,
            },
            requested,
        );
        let mut data = vec![82, 33, 68, 131, 32, 0, 205, 95];
        data.extend_from_slice(&requested.to_le_bytes());

        assert_eq!(instruction.program_id, program::ID);
        assert_eq!(instruction.data, data);
        assert_eq!(
            instruction.accounts,
            [
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(owner, true),
                AccountMeta::new(case.address(), false),
                AccountMeta::new_readonly(case.policy, false),
                AccountMeta::new(case.receipt(&owner), false),
                AccountMeta::new_readonly(program::USDC_MINT, false),
                AccountMeta::new(source, false),
                AccountMeta::new(case.vault(), false),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(system_program::ID, false),
            ]
        );
    }
}
