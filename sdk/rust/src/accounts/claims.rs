use anchor_lang::prelude::Pubkey;
use tekmerion::{CHALLENGE_SIDE, CasePhase, CompletionMethod, StakeReceipt};

use super::{AccountData, CaseView};
use crate::{Result, SdkError};

/// A zero entitlement remains distinct from an unavailable or discharged right.
/// Unclaimed amounts are quotes at the supplied observation, in token base units.
/// A paid receipt records discharge, not the amount of its historical transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimState {
    Unavailable,
    Unclaimed { amount: u64 },
    Claimed,
}

impl ClaimState {
    fn finalized(amount: u64, claimed: bool) -> Self {
        if claimed {
            Self::Claimed
        } else {
            Self::Unclaimed { amount }
        }
    }
}

impl CaseView {
    pub fn author_claim(&self) -> ClaimState {
        if self.case.phase() != CasePhase::Finalized {
            return ClaimState::Unavailable;
        }
        ClaimState::finalized(self.case.author_entitlement, self.author_receipt.claimed)
    }

    /// Total fee already transferred to the winning quorum during settlement.
    /// There is no separate auditor fee claim or unpaid auditor entitlement.
    pub fn audit_fees_paid(&self) -> Option<u64> {
        (self.case.completion_method() == Some(CompletionMethod::Audited))
            .then_some(self.case.fee_entitlement)
    }

    /// Authenticate the owner's canonical receipt before calculating any right.
    /// Pass only receipts from the same observation as this view, even after claim.
    /// For No, the last claimant receives the remaining pool: another claim can
    /// increase this quote before execution. Claimed receipts have no current quote.
    pub fn contributor_claim(&self, owner: Pubkey, account: AccountData<'_>) -> Result<ClaimState> {
        let receipt: StakeReceipt =
            account.decode(self.observation, tekmerion::ID, StakeReceipt::SPACE)?;
        let (address, bump) = StakeReceipt::address(&self.address.address(), &owner);
        if account.key != address
            || receipt.case != self.address.address()
            || receipt.owner != owner
            || receipt.bump != bump
        {
            return Err(SdkError::InvalidRelationship);
        }
        if owner == self.case.author
            || receipt.side != CHALLENGE_SIDE
            || receipt.principal == 0
            || receipt.principal > self.case.challenge_principal
        {
            return Err(SdkError::InvalidState);
        }
        if self.case.phase() != CasePhase::Finalized {
            return if receipt.claimed {
                Err(SdkError::InvalidState)
            } else {
                Ok(ClaimState::Unavailable)
            };
        }
        if receipt.claimed {
            return if self.case.unclaimed_contributors
                > self.case.challenge_principal - receipt.principal
            {
                Err(SdkError::InvalidState)
            } else {
                Ok(ClaimState::Claimed)
            };
        }
        if self.case.unclaimed_contributors == 0
            || self.case.unclaimed_contributors - 1
                > self.case.challenge_principal - receipt.principal
        {
            return Err(SdkError::InvalidState);
        }
        let amount = self
            .case
            .contributor_claim(receipt.principal)
            .map_err(|_| SdkError::Arithmetic)?;
        if u128::from(amount) > self.case.contributor_unpaid
            || (self.case.unclaimed_contributors == 1
                && u128::from(amount) != self.case.contributor_unpaid)
            || (self.case.contributor_entitlement != 0
                && (amount < receipt.principal
                    || self.case.contributor_unpaid - u128::from(amount)
                        < u128::from(self.case.unclaimed_contributors - 1)))
        {
            return Err(SdkError::InvalidState);
        }
        Ok(ClaimState::Unclaimed { amount })
    }
}
