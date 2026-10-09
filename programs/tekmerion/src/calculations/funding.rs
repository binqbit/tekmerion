//! Calculations over a funding snapshot supplied by an authenticated caller.

use super::{
    DomainError,
    amounts::UsdcAmount,
    time::{ChainDeadline, ChainTimestamp},
};

/// Immutable calculation inputs, not persisted Case state or proof of its phase.
///
/// Construction checks only that accepted principal does not exceed the target.
/// In particular, it does not admit a Policy or authenticate time/actual credit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FundingSnapshot {
    target: UsdcAmount,
    accepted: UsdcAmount,
    deadline: ChainDeadline,
}

impl FundingSnapshot {
    /// Check the principal invariant without choosing Policy admission bounds.
    pub fn new(
        target: UsdcAmount,
        accepted: UsdcAmount,
        deadline: ChainDeadline,
    ) -> Result<Self, DomainError> {
        if accepted > target {
            return Err(DomainError::ChallengeAboveTarget);
        }

        Ok(Self {
            target,
            accepted,
            deadline,
        })
    }

    /// Challenge target supplied by the caller.
    pub const fn target(self) -> UsdcAmount {
        self.target
    }

    /// Already accepted principal supplied by the caller.
    pub const fn accepted(self) -> UsdcAmount {
        self.accepted
    }

    /// Existing funding deadline; quoting never moves it.
    pub const fn deadline(self) -> ChainDeadline {
        self.deadline
    }

    /// Whether the numeric target is reached, including the mathematical zero case.
    ///
    /// This is not evidence of a legitimate funded transition or audit eligibility.
    pub fn is_funded(self) -> bool {
        self.accepted == self.target
    }

    /// Calculate the permitted credit and unaccepted remainder before the cutoff.
    ///
    /// The caller must revalidate canonical state/phase and actual token credits
    /// atomically. A stale quote must never be applied as an instruction result.
    /// Zero requests are mathematical no-ops, not an approved onchain operation.
    pub fn quote_contribution(
        self,
        requested: UsdcAmount,
        now: ChainTimestamp,
    ) -> Result<ContributionQuote, DomainError> {
        if self.is_funded() {
            return Err(DomainError::ChallengeAlreadyFunded);
        }
        if !self.deadline.is_open_at(now) {
            return Err(DomainError::FundingClosed);
        }

        let remaining = self.target.checked_sub(self.accepted)?;
        let accepted = requested.min(remaining);
        let resulting_principal = self.accepted.checked_add(accepted)?;

        Ok(ContributionQuote {
            accepted,
            unaccepted: requested.checked_sub(accepted)?,
            resulting_principal,
            reaches_target: resulting_principal == self.target,
        })
    }

    /// Require elapsed funding time and an unmet target for refund calculation.
    ///
    /// A funded snapshot remains ineligible even after its funding deadline.
    /// Callers still must check authorization, live phase and single finalization.
    pub fn require_optimistic_closure(self, now: ChainTimestamp) -> Result<(), DomainError> {
        if self.is_funded() {
            return Err(DomainError::ChallengeAlreadyFunded);
        }
        if !self.deadline.is_elapsed_at(now) {
            return Err(DomainError::FundingStillOpen);
        }

        Ok(())
    }
}

/// A proposed contribution's arithmetic; it is not a receipt or transfer result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContributionQuote {
    accepted: UsdcAmount,
    unaccepted: UsdcAmount,
    resulting_principal: UsdcAmount,
    reaches_target: bool,
}

impl ContributionQuote {
    /// Amount permitted to be credited, subject to actual transfer verification.
    pub const fn accepted(self) -> UsdcAmount {
        self.accepted
    }

    /// Requested amount that must not be collected as this contribution.
    pub const fn unaccepted(self) -> UsdcAmount {
        self.unaccepted
    }

    /// Principal that would result if exactly the permitted amount were credited.
    pub const fn resulting_principal(self) -> UsdcAmount {
        self.resulting_principal
    }

    /// Whether that proposed credit would reach the target.
    pub const fn reaches_target(self) -> bool {
        self.reaches_target
    }
}
