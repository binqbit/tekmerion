//! Errors from pure domain checks; no program error encoding is selected.

use core::fmt;

/// A failed arithmetic or funding predicate.
///
/// These variants have no assigned ABI codes or serialized representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// A sum exceeds its internal amount or liability representation.
    AmountOverflow,
    /// Subtracting an amount would produce a negative result.
    AmountUnderflow,
    /// A seconds conversion or timestamp addition exceeds its representation.
    TimeOverflow,
    /// Recorded challenge principal exceeds the challenge target.
    ChallengeAboveTarget,
    /// The funding deadline has been reached or passed.
    FundingClosed,
    /// Challenge principal has already reached its target.
    ChallengeAlreadyFunded,
    /// The funding deadline has not yet been reached.
    FundingStillOpen,
    /// A funded reward calculation requires a positive challenge target.
    ZeroChallengeTarget,
    /// The fixed audit fee exceeds actual author or challenge collateral.
    InvalidAuditFee,
    /// A contributor claim is inconsistent with the remaining funded allocation.
    InvalidRewardAllocation,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::AmountOverflow => "amount arithmetic overflow",
            Self::AmountUnderflow => "amount arithmetic underflow",
            Self::TimeOverflow => "chain-time arithmetic overflow",
            Self::ChallengeAboveTarget => "challenge principal exceeds the target",
            Self::FundingClosed => "funding deadline has been reached",
            Self::ChallengeAlreadyFunded => "challenge target has already been reached",
            Self::FundingStillOpen => "funding deadline has not been reached",
            Self::ZeroChallengeTarget => "funded allocation requires a positive challenge target",
            Self::InvalidAuditFee => "audit fee exceeds collateral on a possible paying side",
            Self::InvalidRewardAllocation => {
                "contributor claim is inconsistent with the remaining allocation"
            }
        };
        formatter.write_str(message)
    }
}

impl core::error::Error for DomainError {}
