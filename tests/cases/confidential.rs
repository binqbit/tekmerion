use crate::support::{Actor, ExpectedError, confidential::ConfidentialContext, pk};
use anchor_lang::{AccountSerialize, InstructionData, error::ErrorCode};
use root::program::errors::AdapterError;
use tekmerion::{CasePhase, CaseResult};
use tekmerion_chainlink_sdk::{
    self as root,
    confidential::{
        self as sdk,
        payload::{AuditReport, ReportBody},
        state::ConfidentialState,
    },
};
fn error(e: AdapterError) -> ExpectedError {
    ExpectedError::Custom(e as u32 + 6000)
}

fn policy_creation(
    c: &ConfidentialContext,
    identifier: u64,
) -> tekmerion_sdk::instructions::CreatePolicy {
    let admin = pk(c.base.bank.actor(Actor::Admin));
    let policy = tekmerion::Policy::address(&admin, identifier).0;
    let mut rules = c.base.bank.policy_state().rules;
    rules.auditors[0].authority = sdk::ConfidentialProfile::address(&policy).0;
    tekmerion_sdk::instructions::CreatePolicy {
        payer: pk(c.base.bank.actor(Actor::Payer)),
        admin,
        client_authority: pk(c.base.bank.authority),
        identifier,
        rules,
    }
}

#[test]
fn confidential_policy_and_profile_are_created_atomically_and_cannot_be_reconfigured() {
    let mut c = ConfidentialContext::configured(3);
    let input = policy_creation(&c, 1);
    let policy_key = tekmerion::Policy::address(&input.admin, input.identifier).0;
    let profile_key = sdk::ConfidentialProfile::address(&policy_key).0;
    let instructions = sdk::create_policy(input, c.config.clone());
    c.base.bank.ok(&instructions, &[Actor::Admin]);
    let account = c.base.bank.account(profile_key);
    let policy_account = c.base.bank.account(policy_key);
    let policy = <tekmerion::Policy as anchor_lang::AccountDeserialize>::try_deserialize(
        &mut policy_account.data.as_slice(),
    )
    .unwrap();
    let profile = sdk::read_profile(
        profile_key,
        account.owner,
        &account.data,
        policy_key,
        &policy,
    )
    .unwrap();
    assert_eq!(profile.config, c.config);
    assert_eq!(policy.rules.auditors[0].authority, profile_key);

    for field in 0..4 {
        let mut replacement = c.config.clone();
        match field {
            0 => replacement.forwarder_program = anchor_lang::system_program::ID,
            1 => replacement.forwarder_state = pk(c.base.bank.actor(Actor::Admin)),
            2 => replacement.workflow_id = [0x77; 32],
            _ => replacement.agent_config_hash = [0x77; 32],
        }
        let overwrite = sdk::register_profile(
            pk(c.base.bank.actor(Actor::Payer)),
            pk(c.base.bank.actor(Actor::Admin)),
            policy_key,
            replacement,
        );
        assert!(
            c.base
                .bank
                .send(&[overwrite], &[Actor::Admin], &[])
                .is_err()
        );
        assert_eq!(c.base.bank.account(profile_key), account);
    }
}

#[test]
fn confidential_profile_accepts_an_independent_forwarder_state_and_workflow() {
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_signer::Signer;

    let mut c = ConfidentialContext::configured(3);
    let state = Keypair::new_from_array([0xf2; 32]);
    let initialize = Instruction {
        program_id: c.config.forwarder_program,
        accounts: vec![
            AccountMeta::new(state.pubkey(), true),
            AccountMeta::new(pk(c.base.bank.actor(Actor::Admin)), true),
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
        ],
        data: crate::support::chainlink::discriminator("initialize"),
    };
    c.base
        .bank
        .send(&[initialize], &[Actor::Admin], &[&state])
        .unwrap();
    let mut config = c.config.clone();
    config.forwarder_state = state.pubkey();
    config.workflow_id = [0x44; 32];
    let input = policy_creation(&c, 3);
    let profile_key = sdk::ConfidentialProfile::address(
        &tekmerion::Policy::address(&input.admin, input.identifier).0,
    )
    .0;
    c.base
        .bank
        .ok(&sdk::create_policy(input, config.clone()), &[Actor::Admin]);
    let account = c.base.bank.account(profile_key);
    let profile = <sdk::ConfidentialProfile as anchor_lang::AccountDeserialize>::try_deserialize(
        &mut account.data.as_slice(),
    )
    .unwrap();
    assert_eq!(profile.config, config);
}

#[test]
fn confidential_profile_does_not_require_a_known_forwarder_program() {
    let mut c = ConfidentialContext::configured(3);
    let mut config = c.config.clone();
    // An existing executable and its owned account exercise generic enrollment.
    // This does not assert that the token program implements report delivery.
    config.forwarder_program = pk(crate::support::TOKEN);
    config.forwarder_state = pk(c.base.bank.token(Actor::Author));
    let input = policy_creation(&c, 4);
    let profile_key = sdk::ConfidentialProfile::address(
        &tekmerion::Policy::address(&input.admin, input.identifier).0,
    )
    .0;
    c.base
        .bank
        .ok(&sdk::create_policy(input, config.clone()), &[Actor::Admin]);
    let account = c.base.bank.account(profile_key);
    let profile = <sdk::ConfidentialProfile as anchor_lang::AccountDeserialize>::try_deserialize(
        &mut account.data.as_slice(),
    )
    .unwrap();
    assert_eq!(profile.config, config);
}

#[test]
fn confidential_profile_rejects_invalid_program_or_state_owner() {
    let mut c = ConfidentialContext::configured(3);
    let input = policy_creation(&c, 5);
    let mut instructions = sdk::create_policy(input, c.config.clone());
    let program = instructions[1]
        .accounts
        .iter_mut()
        .find(|account| account.pubkey == c.config.forwarder_program)
        .unwrap();
    program.pubkey = anchor_lang::system_program::ID;
    c.base.bank.reject_at(
        &instructions,
        &[Actor::Admin],
        1,
        ExpectedError::Anchor(ErrorCode::ConstraintAddress),
    );

    let input = policy_creation(&c, 6);
    let mut config = c.config.clone();
    config.forwarder_state = pk(c.base.bank.actor(Actor::Admin));
    c.base.bank.reject_at(
        &sdk::create_policy(input, config),
        &[Actor::Admin],
        1,
        ExpectedError::Anchor(ErrorCode::ConstraintOwner),
    );

    let input = policy_creation(&c, 7);
    let mut config = c.config.clone();
    config.forwarder_program = pk(c.base.bank.actor(Actor::Admin));
    c.base.bank.reject_at(
        &sdk::create_policy(input, config),
        &[Actor::Admin],
        1,
        ExpectedError::Anchor(ErrorCode::ConstraintExecutable),
    );
}

#[test]
fn confidential_invalid_configuration_rolls_back_policy_profile_and_rent() {
    let mut c = ConfidentialContext::configured(3);
    let mut config = c.config.clone();
    config.max_attempts = 0;
    let input = policy_creation(&c, 2);
    let policy = tekmerion::Policy::address(&input.admin, input.identifier).0;
    let profile = sdk::ConfidentialProfile::address(&policy).0;
    c.base.bank.reject_at(
        &sdk::create_policy(input, config),
        &[Actor::Admin],
        1,
        error(AdapterError::InvalidConfiguration),
    );
    assert!(c.base.bank.get_account(&policy).is_none());
    assert!(c.base.bank.get_account(&profile).is_none());
}

#[test]
fn confidential_cca_009_claim_is_required_and_competing_execution_loses() {
    let mut c = ConfidentialContext::funded(3);
    let verdict = c.verdict(0, 1, 0, 0);
    c.base.bank.reject_at(
        &c.report_instructions(&verdict),
        &[],
        1,
        error(AdapterError::InvalidAttempt),
    );
    c.claim(0, 2);
    let other = c.report_value(0, 3, ReportBody::InputAccepted);
    c.base.bank.reject_at(
        &c.report_instructions(&other),
        &[],
        1,
        error(AdapterError::ConflictingReport),
    );
    assert_eq!(c.request().attempts_started, 1);
    assert_eq!(c.request().execution_id, [2; 32]);
    assert_eq!(c.base.bank.case_state().phase(), CasePhase::Funded);
}

#[test]
fn confidential_cca_011_attested_failure_allows_next_attempt_with_the_same_task() {
    let mut c = ConfidentialContext::funded(3);
    let digest = c.request().digest;
    c.claim(0, 1);
    let fail = c.report_value(
        0,
        1,
        ReportBody::AttemptFailed {
            code: 2,
            report_hash: [0x77; 32],
        },
    );
    c.base.bank.ok(&c.report_instructions(&fail), &[]);
    assert_eq!(c.request().state, ConfidentialState::Retryable);
    c.claim(1, 2);
    assert_eq!(c.request().digest, digest);
    assert_eq!(c.request().attempts_started, 2);
    let verdict = c.verdict(1, 2, 0, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);
    assert_eq!(c.request().state, ConfidentialState::Ready);
    c.base.bank.ok(
        &[
            tekmerion_chainlink_sdk::confidential::compute_budget(),
            c.apply(),
        ],
        &[],
    );
    assert_eq!(c.request().state, ConfidentialState::Applied);
    assert_eq!(c.base.bank.case_state().result(), Some(CaseResult::Yes));
    assert_eq!(c.base.bank.case_state().fee_entitlement, 7);
    // A historical failure is rejected without changing the later result.
    c.base.bank.reject_at(
        &c.report_instructions(&fail),
        &[],
        1,
        ExpectedError::Custom(6008),
    );
    assert_eq!(c.request().state, ConfidentialState::Applied);
}

#[test]
fn confidential_cca_013_limits_and_nonretryable_failures_stop_fresh_evaluations() {
    for (limit, code) in [(1, 2), (3, 67)] {
        let mut c = ConfidentialContext::funded(limit);
        c.claim(0, 1);
        let fail = c.report_value(
            0,
            1,
            ReportBody::AttemptFailed {
                code,
                report_hash: [0x77; 32],
            },
        );
        c.base.bank.ok(&c.report_instructions(&fail), &[]);
        assert_eq!(c.request().state, ConfidentialState::Stopped);
        let retry = c.report_value(1, 2, ReportBody::InputAccepted);
        let ix = c.report(&retry);
        c.base.bank.reject_at(
            &[sdk::compute_budget(), ix],
            &[],
            1,
            error(AdapterError::InvalidAttempt),
        );
    }
}

#[test]
fn confidential_cca_014_yes_no_and_unknown_apply_once_without_semantic_retries() {
    for (answer, reason, result) in [
        (0, 0, CaseResult::Yes),
        (1, 0, CaseResult::No),
        (2, 5, CaseResult::Unknown),
    ] {
        let mut c = ConfidentialContext::funded(3);
        c.claim(0, 1);
        let p = c.verdict(0, 1, answer, reason);
        c.base.bank.ok(&c.report_instructions(&p), &[]);
        c.base.bank.ok(
            &[
                tekmerion_chainlink_sdk::confidential::compute_budget(),
                c.apply(),
            ],
            &[],
        );
        c.base.bank.ok(
            &[
                tekmerion_chainlink_sdk::confidential::compute_budget(),
                c.apply(),
            ],
            &[],
        );
        assert_eq!(c.base.bank.case_state().result(), Some(result));
        assert_eq!(c.base.bank.case_state().audit.votes.len(), 1);
        let retry = c.report_value(1, 2, ReportBody::InputAccepted);
        let ix = c.report(&retry);
        c.base.bank.reject_at(
            &[sdk::compute_budget(), ix],
            &[],
            1,
            error(AdapterError::AuditClosed),
        );
    }
}

#[test]
fn confidential_retransmissions_preserve_terminal_state_and_custody() {
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, 1);
    let claim = c.report_value(0, 1, ReportBody::InputAccepted);
    let verdict = c.verdict(0, 1, 1, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);

    let protected = [
        c.base.request,
        pk(c.base.bank.case),
        pk(c.base.bank.vault),
        pk(c.base.bank.token(Actor::Author)),
        pk(c.base.bank.token(Actor::FeeRecipient)),
    ];
    for applied in [false, true] {
        if applied {
            c.base.bank.ok(&[sdk::compute_budget(), c.apply()], &[]);
            let deadline = c.base.bank.case_state().audit.deadline.unwrap();
            c.base.bank.set_time(deadline + 1);
        }
        let before = protected.map(|key| c.base.bank.account(key));
        let delivery = if applied { 20 } else { 10 };
        for payload in [&claim, &verdict] {
            // Fresh native transmissions reach the adapter's duplicate guard,
            // instead of stopping at the Forwarder's own replay protection.
            let ix = c.raw_report(payload, &payload.encode().unwrap(), delivery);
            c.base.bank.ok(&[sdk::compute_budget(), ix], &[]);
            assert_eq!(protected.map(|key| c.base.bank.account(key)), before);
        }

        let conflicting = c.verdict(0, 1, 0, 0);
        let ix = c.raw_report(&conflicting, &conflicting.encode().unwrap(), delivery + 1);
        c.base.bank.reject_at(
            &[sdk::compute_budget(), ix],
            &[],
            1,
            error(AdapterError::ConflictingReport),
        );
        assert_eq!(protected.map(|key| c.base.bank.account(key)), before);
    }
    assert_eq!(c.base.bank.case_state().result(), Some(CaseResult::No));
    assert_eq!(c.base.bank.case_state().audit.votes.len(), 1);
}

#[test]
fn confidential_cca_002_a_signed_wrong_task_is_rejected_after_real_signature_verification() {
    let mut c = ConfidentialContext::funded(3);
    let mut p = c.report_value(0, 1, ReportBody::InputAccepted);
    p.prefix.request_digest[0] ^= 1;
    c.base.bank.reject_at(
        &c.report_instructions(&p),
        &[],
        1,
        error(AdapterError::InvalidBinding),
    );
    assert_eq!(c.request().state, ConfidentialState::Prepared);
    assert_eq!(c.request().attempts_started, 0);
}

#[test]
fn confidential_cca_006_unused_policy_edits_break_the_immutable_profile_binding() {
    let mut c = ConfidentialContext::enrolled(3);
    let config_hash = c.profile().config.agent_config_hash;
    let mut rules = c.base.bank.policy_state().rules;
    rules.audit_timeout += 1;
    c.base
        .bank
        .ok(&[c.base.bank.update_policy_ix(rules)], &[Actor::Admin]);
    c.base.bank.create_case();
    c.base.bank.contribute(Actor::Alice, crate::support::TARGET);
    let ix = sdk::prepare_request(
        pk(c.base.bank.actor(Actor::Payer)),
        c.base.bank.case_address(),
    );
    c.base
        .bank
        .reject(&[ix], &[], error(AdapterError::InvalidBinding));
    assert_eq!(c.profile().config.agent_config_hash, config_hash);
}

#[test]
fn confidential_cca_017_deadline_does_not_extend_when_retrying() {
    let mut c = ConfidentialContext::funded(3);
    let deadline = c.base.bank.case_state().audit.deadline.unwrap();
    c.claim(0, 1);
    c.base.bank.set_time(deadline);
    let p = c.verdict(0, 1, 0, 0);
    c.base.bank.reject_at(
        &c.report_instructions(&p),
        &[],
        1,
        error(AdapterError::AuditClosed),
    );
    c.base.bank.ok(&[c.base.bank.finalize_timeout_ix()], &[]);
    assert_eq!(c.base.bank.case_state().result(), None);
    assert_eq!(c.base.bank.case_state().fee_entitlement, 0);
}

#[test]
fn confidential_cca_003_strict_report_sizes_codes_and_hashes() {
    let c = ConfidentialContext::funded(3);
    let p = c.report_value(0, 1, ReportBody::InputAccepted);
    let bytes = p.encode().unwrap();
    assert_eq!(bytes.len(), 68);
    assert_eq!(AuditReport::decode(&bytes).unwrap(), p);
    assert!(AuditReport::decode(&bytes[..67]).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(AuditReport::decode(&extra).is_err());
    let verdict = c.verdict(0, 1, 0, 0);
    let mut bytes = verdict.encode().unwrap();
    assert_eq!(bytes.len(), 102);
    assert_eq!(AuditReport::decode(&bytes).unwrap(), verdict);
    bytes.extend([0x55; 32]);
    assert!(AuditReport::decode(&bytes).is_err());
    assert_eq!(c.base.bank.account(c.base.profile).data.len(), 204);
    assert_eq!(c.base.bank.account(c.base.request).data.len(), 232);
}

#[test]
fn confidential_cca_016_fee_cpi_failure_leaves_the_same_verdict_ready() {
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, 1);
    let verdict = c.verdict(0, 1, 0, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);
    let mut apply = c.apply();
    apply.accounts.last_mut().unwrap().pubkey = pk(c.base.bank.token(Actor::Outsider));
    c.base.bank.reject_at(
        &[sdk::compute_budget(), apply],
        &[],
        1,
        ExpectedError::Protocol(tekmerion::errors::TekmerionError::InvalidFeeRecipient),
    );
    assert_eq!(c.request().state, ConfidentialState::Ready);
    assert!(c.base.bank.case_state().audit.votes.is_empty());
    c.base.bank.ok(&[sdk::compute_budget(), c.apply()], &[]);
    assert_eq!(c.request().state, ConfidentialState::Applied);
}

#[test]
fn confidential_cca_007_wallet_cannot_claim_or_attest_a_retry() {
    let mut c = ConfidentialContext::funded(3);
    let report = c.report_value(0, 1, ReportBody::InputAccepted);
    let mut accounts = sdk::receiver_accounts(c.base.bank.case_address(), &c.config);
    accounts[1].pubkey = pk(c.base.bank.actor(Actor::Outsider));
    let ix = solana_instruction::Instruction {
        program_id: root::ID,
        accounts,
        data: root::program::instruction::OnReport {
            metadata: c.metadata(0),
            report: report.encode().unwrap(),
        }
        .data(),
    };
    c.base.bank.reject_at(
        &[sdk::compute_budget(), ix],
        &[Actor::Outsider],
        1,
        error(AdapterError::InvalidForwarder),
    );
    assert_eq!(c.request().state, ConfidentialState::Prepared);
    assert_eq!(c.request().attempts_started, 0);
}

#[test]
fn confidential_authentication_rejects_wrong_workflow_and_invalid_oracle_signature() {
    let mut c = ConfidentialContext::funded(3);
    let payload = c.report_value(0, 1, ReportBody::InputAccepted);
    let expected = c.config.workflow_id;
    c.config.workflow_id[0] ^= 1;
    let ix = c.report(&payload);
    c.config.workflow_id = expected;
    c.base.bank.reject_at(
        &[sdk::compute_budget(), ix],
        &[],
        1,
        error(AdapterError::InvalidWorkflow),
    );
    let mut invalid = c.report(&payload);
    // Anchor discriminator + Borsh Vec length + signature count precede r/s.
    invalid.data[13..77].fill(0);
    c.base.bank.reject_at(
        &[sdk::compute_budget(), invalid],
        &[],
        1,
        ExpectedError::Custom(6005),
    );
    assert_eq!(c.request().state, ConfidentialState::Prepared);
    assert_eq!(c.request().attempts_started, 0);
    // Rejected transmissions do not consume the valid execution's claim.
    c.claim(0, 1);
    assert_eq!(c.request().state, ConfidentialState::Active);
}

#[test]
fn confidential_sdk_request_reader_rejects_unbound_policy_and_configuration() {
    let c = ConfidentialContext::funded(3);
    let profile = c.profile();
    let policy = c.base.bank.policy_state();
    let account = c.base.bank.account(c.base.request);
    let read = |policy: &tekmerion::Policy, profile: &sdk::ConfidentialProfile| {
        sdk::read_request(
            c.base.request,
            account.owner,
            &account.data,
            sdk::RequestBinding {
                profile_key: c.base.profile,
                profile,
                case_key: pk(c.base.bank.case),
                case: &c.base.bank.case_state(),
                policy,
            },
        )
    };
    read(&policy, &profile).unwrap();
    let mut changed_profile = c.profile();
    changed_profile.config.agent_config_hash[0] ^= 1;
    assert!(read(&policy, &changed_profile).is_err());
    let mut changed_profile = c.profile();
    changed_profile.config.forwarder_program = pk(crate::support::TOKEN);
    assert!(read(&policy, &changed_profile).is_err());
    let mut changed_profile = c.profile();
    changed_profile.config.forwarder_state = pk(c.base.bank.actor(Actor::Admin));
    assert!(read(&policy, &changed_profile).is_err());
    let mut changed_policy = c.base.bank.policy_state();
    changed_policy.rules.audit_fee += 1;
    assert!(read(&changed_policy, &profile).is_err());
    let mut changed_case = c.base.bank.case_state();
    changed_case.author = pk(c.base.bank.actor(Actor::Outsider));
    assert!(
        sdk::binding::request_digest(
            c.base.profile,
            &profile,
            pk(c.base.bank.case),
            &changed_case
        )
        .is_err()
    );
}

#[test]
fn confidential_sdk_request_reader_rejects_application_at_or_after_deadline() {
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, 1);
    let verdict = c.verdict(0, 1, 0, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);
    c.base.bank.ok(&[sdk::compute_budget(), c.apply()], &[]);

    let profile = c.profile();
    let policy = c.base.bank.policy_state();
    let mut case = c.base.bank.case_state();
    let mut request = c.request();
    let owner = c.base.bank.account(c.base.request).owner;
    let read = |request: &sdk::ConfidentialRequest, case: &tekmerion::Case| {
        let mut bytes = Vec::new();
        request.try_serialize(&mut bytes).unwrap();
        sdk::read_request(
            c.base.request,
            owner,
            &bytes,
            sdk::RequestBinding {
                profile_key: c.base.profile,
                profile: &profile,
                case_key: pk(c.base.bank.case),
                case,
                policy: &policy,
            },
        )
    };
    read(&request, &case).unwrap();
    let deadline = case.audit.deadline.unwrap();
    // Matching timestamps in both accounts must still respect the audit cutoff.
    request.applied_at = deadline - 1;
    case.finalized_at = Some(request.applied_at);
    read(&request, &case).unwrap();
    for applied_at in [deadline, deadline + 1] {
        request.applied_at = applied_at;
        case.finalized_at = Some(applied_at);
        assert!(read(&request, &case).is_err());
    }
}

#[test]
fn confidential_old_execution_cannot_finish_a_new_attempt() {
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, 1);
    let failed = c.report_value(
        0,
        1,
        ReportBody::AttemptFailed {
            code: 2,
            report_hash: [7; 32],
        },
    );
    c.base.bank.ok(&c.report_instructions(&failed), &[]);
    c.claim(1, 2);
    let stale = c.verdict(0, 1, 0, 0);
    // A new native transmission still cannot revive the obsolete inner execution.
    let stale_ix = c.raw_report(&stale, &stale.encode().unwrap(), 3);
    c.base.bank.reject_at(
        &[sdk::compute_budget(), stale_ix],
        &[],
        1,
        error(AdapterError::InvalidAttempt),
    );
    assert_eq!(c.request().execution_id, [2; 32]);
    assert_eq!(c.request().state, ConfidentialState::Active);
}

#[test]
fn confidential_ready_verdict_cannot_apply_after_core_timeout() {
    let mut c = ConfidentialContext::funded(3);
    c.claim(0, 1);
    let verdict = c.verdict(0, 1, 0, 0);
    c.base.bank.ok(&c.report_instructions(&verdict), &[]);
    let deadline = c.base.bank.case_state().audit.deadline.unwrap();
    c.base.bank.set_time(deadline);
    c.base
        .bank
        .reject(&[c.apply()], &[], error(AdapterError::AuditClosed));
    assert_eq!(c.request().state, ConfidentialState::Ready);
    assert!(c.base.bank.case_state().audit.votes.is_empty());
    c.base.bank.ok(&[c.base.bank.finalize_timeout_ix()], &[]);
    assert_eq!(c.base.bank.case_state().fee_entitlement, 0);
}
