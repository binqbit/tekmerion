use anchor_lang::prelude::*;

use super::{Case, CaseResult, CompletionMethod};
use crate::calculations::{
    amounts::{LiabilityTotal, UsdcAmount},
    settlement,
};
use crate::errors::{TekmerionError, domain_error};

/// Prepared against the current Case before payment; recorded only after CPI.
/// These transient values are not part of the serialized account.
pub(crate) struct ContributorClaim {
    amount: u64,
    remaining_liability: u128,
    remaining_contributors: u64,
}

impl ContributorClaim {
    pub(crate) fn amount(&self) -> u64 {
        self.amount
    }
}

impl Case {
    /// Quote an unclaimed receipt against current state. The last No claimant
    /// receives the remaining contributor liability, including rounding dust.
    /// Callers authenticate the unclaimed receipt. Audit fees are frozen in Case.
    pub fn contributor_claim(&self, principal: u64) -> Result<u64> {
        self.require_finalized()?;
        require!(
            self.unclaimed_contributors > 0
                && principal > 0
                && principal <= self.challenge_principal,
            TekmerionError::InvalidReceipt
        );

        if self.completion_method() != Some(CompletionMethod::Audited) {
            return Ok(principal);
        }

        match self.result {
            Some(CaseResult::Yes) => Ok(0),
            Some(CaseResult::Unknown) => Ok(principal),
            Some(CaseResult::No) => {
                let principal = UsdcAmount::from_base_units(principal);
                let reward_pool = self.no_reward_pool()?;
                let total_principal = UsdcAmount::from_base_units(self.challenge_target);

                let remaining_base_units = self
                    .contributor_unpaid
                    .try_into()
                    .map_err(|_| TekmerionError::ArithmeticOverflow)?;
                let remaining_liability =
                    LiabilityTotal::from_amount(UsdcAmount::from_base_units(remaining_base_units));
                let is_last_contributor = self.unclaimed_contributors == 1;

                settlement::no_contributor_claim(
                    principal,
                    reward_pool,
                    total_principal,
                    remaining_liability,
                    is_last_contributor,
                )
                .map(UsdcAmount::base_units)
                .map_err(domain_error)
            }
            None => err!(TekmerionError::InvalidPhase),
        }
    }

    fn no_reward_pool(&self) -> Result<UsdcAmount> {
        let author_principal = UsdcAmount::from_base_units(self.author_principal);
        let audit_fee = UsdcAmount::from_base_units(self.fee_entitlement);

        author_principal
            .checked_sub(audit_fee)
            .map_err(domain_error)
    }

    // The caller obtains author_claimed from the authenticated author receipt.
    pub fn outstanding_liability(&self, author_claimed: bool) -> u128 {
        let author_unpaid = if author_claimed {
            0
        } else {
            self.author_entitlement
        };
        let contributors_unpaid = self.contributor_unpaid;
        let auditors_unpaid = if self.fee_claimed {
            0
        } else {
            self.fee_entitlement
        };

        u128::from(author_unpaid) + contributors_unpaid + u128::from(auditors_unpaid)
    }

    /// The caller obtains `author_claimed` from the authenticated author receipt.
    pub fn unpaid(&self, author_claimed: bool) -> u128 {
        self.outstanding_liability(author_claimed)
    }

    pub(crate) fn prepare_contributor_claim(&self, principal: u64) -> Result<ContributorClaim> {
        let amount = self.contributor_claim(principal)?;

        let remaining_liability = self
            .contributor_unpaid
            .checked_sub(u128::from(amount))
            .ok_or(TekmerionError::InsufficientBacking)?;
        let remaining_contributors = self
            .unclaimed_contributors
            .checked_sub(1)
            .ok_or(TekmerionError::InvalidReceipt)?;

        require!(
            remaining_contributors != 0 || remaining_liability == 0,
            TekmerionError::InsufficientBacking
        );

        Ok(ContributorClaim {
            amount,
            remaining_liability,
            remaining_contributors,
        })
    }

    /// Discharge exactly the claim prepared before the successful payment.
    pub(crate) fn record_contributor_claim(&mut self, claim: ContributorClaim) {
        self.contributor_unpaid = claim.remaining_liability;
        self.unclaimed_contributors = claim.remaining_contributors;
    }
}
