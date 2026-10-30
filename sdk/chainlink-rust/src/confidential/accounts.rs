//! Strict account reads validate ownership, immutable task identity and lifecycle.
use super::{ConfidentialProfile, ConfidentialRequest, ConfidentialState, binding};
use crate::program;
use anchor_lang::prelude::*;
use program::errors::AdapterError;
use tekmerion_sdk::program::{Case, Policy};

pub fn read_profile(
    key: Pubkey,
    owner: Pubkey,
    bytes: &[u8],
    policy_key: Pubkey,
    policy: &Policy,
) -> Result<ConfidentialProfile> {
    require!(
        owner == program::ID && bytes.len() == 8 + ConfidentialProfile::INIT_SPACE,
        AdapterError::InvalidProfile
    );
    let profile = ConfidentialProfile::try_deserialize(&mut &bytes[..])?;
    binding::validate_profile(key, &profile, policy_key, policy)?;
    Ok(profile)
}

pub struct RequestBinding<'a> {
    pub profile_key: Pubkey,
    pub profile: &'a ConfidentialProfile,
    pub case_key: Pubkey,
    pub case: &'a Case,
    pub policy: &'a Policy,
}

pub fn read_request(
    key: Pubkey,
    owner: Pubkey,
    bytes: &[u8],
    binding: RequestBinding<'_>,
) -> Result<ConfidentialRequest> {
    let RequestBinding {
        profile_key,
        profile,
        case_key,
        case,
        policy,
    } = binding;
    require!(
        owner == program::ID && bytes.len() == 8 + ConfidentialRequest::INIT_SPACE,
        AdapterError::InvalidBinding
    );
    binding::validate_profile(profile_key, profile, case.policy, policy)?;
    let request = ConfidentialRequest::try_deserialize(&mut &bytes[..])?;
    let (expected, bump) = ConfidentialRequest::address(&profile_key, &case_key);
    require!(
        key == expected
            && request.version == program::confidential::VERSION
            && request.bump == bump
            && request.profile == profile_key
            && request.case == case_key
            && request.digest == binding::request_digest(profile_key, profile, case_key, case)?
            && request.attempts_started <= profile.config.max_attempts,
        AdapterError::InvalidBinding
    );
    validate_lifecycle(&request, profile, case)?;
    Ok(request)
}

fn validate_lifecycle(
    request: &ConfidentialRequest,
    profile: &ConfidentialProfile,
    case: &Case,
) -> Result<()> {
    use ConfidentialState::*;
    if request.state == Prepared {
        require!(
            request.attempts_started == 0
                && request.execution_id == [0; 32]
                && request.claimed_at == 0,
            AdapterError::InvalidBinding
        );
    } else {
        require!(
            request.attempts_started > 0
                && request.execution_id != [0; 32]
                && case
                    .audit
                    .deadline
                    .is_some_and(|deadline| request.claimed_at < deadline),
            AdapterError::InvalidBinding
        );
    }
    if matches!(request.state, Prepared | Active) {
        require!(
            request.terminal_at == 0
                && request.terminal_payload_hash == [0; 32]
                && request.failure_code == 0
                && request.report_hash == [0; 32],
            AdapterError::InvalidBinding
        );
    } else {
        require!(
            request.terminal_at >= request.claimed_at
                && request.terminal_payload_hash != [0; 32]
                && request.report_hash != [0; 32]
                && case
                    .audit
                    .deadline
                    .is_some_and(|deadline| request.terminal_at < deadline),
            AdapterError::InvalidBinding
        );
    }
    if matches!(request.state, Ready | Applied) {
        program::payload::decision(request.answer, request.reason)?;
        require!(request.failure_code == 0, AdapterError::InvalidBinding);
    } else {
        require!(
            request.answer == 0 && request.reason == 0 && request.applied_at == 0,
            AdapterError::InvalidBinding
        );
    }
    if matches!(request.state, Retryable | Stopped) {
        let can_retry = program::confidential::payload::retryable(request.failure_code)?
            && request.attempts_started < profile.config.max_attempts;
        require!(
            (request.state == Retryable) == can_retry,
            AdapterError::InvalidBinding
        );
    }
    if request.state == Applied {
        let decision = program::payload::decision(request.answer, request.reason)?;
        let result = match request.answer {
            0 => tekmerion_sdk::program::CaseResult::Yes,
            1 => tekmerion_sdk::program::CaseResult::No,
            2 => tekmerion_sdk::program::CaseResult::Unknown,
            _ => return err!(AdapterError::InvalidBinding),
        };
        require!(
            case.result() == Some(result)
                && case.audit.response == Some(decision)
                && case.finalized_at == Some(request.applied_at)
                && request.applied_at >= request.terminal_at
                && case.completion_method()
                    == Some(tekmerion_sdk::program::CompletionMethod::Audited)
                && case.audit.votes
                    == vec![tekmerion_sdk::program::AuditVote {
                        auditor_index: 0,
                        response: decision
                    }],
            AdapterError::InvalidBinding
        );
    } else {
        require!(request.applied_at == 0, AdapterError::InvalidBinding);
    }
    Ok(())
}
