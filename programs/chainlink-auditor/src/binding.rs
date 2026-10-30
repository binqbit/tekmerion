use crate::errors::AdapterError;
use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;
use tekmerion_sdk::program::{Case, Policy};

pub fn network_domain() -> [u8; 32] {
    #[cfg(feature = "mainnet")]
    let domain = b"tekmerion:chainlink:solana-mainnet:v1";
    #[cfg(all(feature = "devnet", not(feature = "mainnet")))]
    let domain = b"tekmerion:chainlink:solana-devnet:v1";
    hashv(&[domain]).to_bytes()
}

/// Deliberately excludes mutable active_uses and all Case claim balances.
pub fn rules_hash(policy: &Policy) -> Result<[u8; 32]> {
    let mut rules = Vec::new();
    policy.rules.serialize(&mut rules)?;
    Ok(hashv(&[
        b"tekmerion:chainlink:policy:v1",
        policy.client_authority.as_ref(),
        &rules,
    ])
    .to_bytes())
}

/// A technical failure is not a vote; only funded, unexpired Cases accept one.
pub fn require_open(case: &Case, now: i64) -> Result<()> {
    require!(
        case.phase() == tekmerion_sdk::program::CasePhase::Funded
            && case.audit.deadline.is_some_and(|deadline| now < deadline),
        AdapterError::AuditClosed
    );
    Ok(())
}
