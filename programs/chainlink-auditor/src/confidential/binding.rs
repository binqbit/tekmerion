use crate::{errors::AdapterError, state::ConfidentialProfile};
use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;
use tekmerion_sdk::program::{Case, Policy};

pub fn validate_profile(
    key: Pubkey,
    profile: &ConfidentialProfile,
    policy_key: Pubkey,
    policy: &Policy,
) -> Result<()> {
    profile.validate_config()?;
    require!(
        profile.bump == ConfidentialProfile::address(&policy_key).1,
        AdapterError::InvalidBinding
    );
    require_keys_eq!(
        ConfidentialProfile::address(&policy_key).0,
        key,
        AdapterError::InvalidBinding
    );
    require_keys_eq!(profile.policy, policy_key, AdapterError::InvalidBinding);
    require_keys_eq!(
        Policy::address(&policy.admin, policy.identifier).0,
        policy_key,
        AdapterError::InvalidBinding
    );
    require!(
        policy.rules.auditors.len() == 1
            && policy.rules.threshold == 1
            && policy.rules.auditors[0].authority == key
            && profile.rules_hash == crate::binding::rules_hash(policy)?,
        AdapterError::InvalidBinding
    );
    Ok(())
}

pub fn request_digest(
    profile_key: Pubkey,
    profile: &ConfidentialProfile,
    case_key: Pubkey,
    case: &Case,
) -> Result<[u8; 32]> {
    require!(
        case.policy == profile.policy
            && Case::address(&case.policy, &case.subject, &case.author, case.nonce).0 == case_key,
        AdapterError::InvalidBinding
    );
    let bytes = super::serialize(profile)?;
    let deadline = case.audit.deadline.ok_or(AdapterError::AuditClosed)?;
    Ok(hashv(&[
        b"tekmerion:chainlink:request:v3",
        &crate::binding::network_domain(),
        tekmerion_sdk::ID.as_ref(),
        crate::ID.as_ref(),
        profile_key.as_ref(),
        &(bytes.len() as u32).to_le_bytes(),
        &bytes,
        case.policy.as_ref(),
        case_key.as_ref(),
        case.subject.as_ref(),
        &case.nonce.to_le_bytes(),
        &case.agreement_hash,
        &case.result_hash,
        &deadline.to_le_bytes(),
    ])
    .to_bytes())
}
