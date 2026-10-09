use anchor_lang::prelude::Pubkey;
use tekmerion::Policy;

use super::{AccountData, Observation};
use crate::{Result, SdkError};

/// A canonical Policy authenticated independently of any Case.
/// Unused policies are valid; the configured rules may still be updated by admin.
#[derive(Debug)]
pub struct PolicyView {
    observation: Observation,
    address: Pubkey,
    policy: Policy,
}

impl PolicyView {
    pub fn read(
        observation: Observation,
        expected: Pubkey,
        account: AccountData<'_>,
    ) -> Result<Self> {
        let policy: Policy = account.decode(observation, tekmerion::ID, Policy::SPACE)?;
        let (address, bump) = Policy::address(&policy.admin, policy.identifier);
        if account.key != expected || address != expected || policy.bump != bump {
            return Err(SdkError::InvalidRelationship);
        }
        // An existing policy is not a new admission: its durations do not need
        // to produce new deadlines relative to the observation's current time.
        if !policy.rules.has_valid_parameters() {
            return Err(SdkError::InvalidState);
        }

        Ok(Self {
            observation,
            address,
            policy,
        })
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn address(&self) -> Pubkey {
        self.address
    }

    pub fn observation(&self) -> Observation {
        self.observation
    }
}
