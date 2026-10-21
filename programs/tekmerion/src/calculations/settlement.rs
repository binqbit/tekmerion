//! Terminal allocations and exact contributor rewards.
//!
//! These functions calculate amounts. The caller authenticates the lifecycle,
//! auditor quorum, receipts and custody, then records each payment exactly once.

use super::{
    DomainError,
    amounts::{LiabilityTotal, UsdcAmount},
    funding::FundingSnapshot,
    time::ChainTimestamp,
};

/// Aggregate fee-free refund amounts, not a persisted claim ledger.
///
/// Each authenticated contributor receipt is owed its own accepted principal.
/// This aggregate does not authenticate receipt totals, recipients, backing,
/// finality or payment status. The instruction boundary enforces those checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptimisticRefunds {
    author: UsdcAmount,
    contributors: UsdcAmount,
    total: LiabilityTotal,
}

impl OptimisticRefunds {
    /// Entire actual author principal, including any amount above the minimum.
    pub const fn author(self) -> UsdcAmount {
        self.author
    }

    /// Total accepted challenge principal, all refundable to its contributors.
    pub const fn contributors(self) -> UsdcAmount {
        self.contributors
    }

    /// No audit fee is incurred on optimistic acceptance.
    pub const fn audit_fee(self) -> UsdcAmount {
        UsdcAmount::ZERO
    }

    /// Widened sum of all refund obligations, without narrowing or truncation.
    pub const fn total(self) -> LiabilityTotal {
        self.total
    }
}

/// Compute full refunds once the deadline elapses below the challenge target.
///
/// The author amount is actual credited principal, not the Policy minimum.
/// This pure function does not finalize a Case or authorize a payout.
pub fn optimistic_refunds(
    author: UsdcAmount,
    funding: FundingSnapshot,
    now: ChainTimestamp,
) -> Result<OptimisticRefunds, DomainError> {
    funding.require_optimistic_closure(now)?;

    let contributors = funding.accepted();
    let total = LiabilityTotal::from_amount(author)
        .checked_add(LiabilityTotal::from_amount(contributors))?;

    Ok(OptimisticRefunds {
        author,
        contributors,
        total,
    })
}

/// An auditor answer used for allocation, distinct from a technical timeout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditedOutcome {
    /// The assertion is supported; challenger collateral bears the fixed fee.
    Yes,
    /// The assertion is contradicted; actual author collateral bears the fee.
    No,
    /// The auditor cannot decide; the author pays only the fixed fee.
    Unknown,
}

/// Immutable aggregate entitlements calculated before any discharge.
///
/// Contributor totals are widened because several individual recipients can have
/// an aggregate right larger than `u64`. Actual single-account custody and each
/// individual payout must still fit the runtime's token amount representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FundedAllocation {
    author: UsdcAmount,
    contributors: LiabilityTotal,
    audit_fee: UsdcAmount,
    total: LiabilityTotal,
}

impl FundedAllocation {
    /// Entire author entitlement, including any retained excess actual bond.
    pub const fn author(self) -> UsdcAmount {
        self.author
    }

    /// Aggregate rights across all contributor receipts.
    pub const fn contributors(self) -> LiabilityTotal {
        self.contributors
    }

    /// Total fee allocated to the winning auditors; this does not transfer tokens.
    pub const fn audit_fee(self) -> UsdcAmount {
        self.audit_fee
    }

    /// All author, contributor and fee rights, equal to actual collateral inputs.
    pub const fn total(self) -> LiabilityTotal {
        self.total
    }
}

/// Allocate the collateral of a fully funded Case after an auditor quorum.
///
/// Let actual author principal be A, funded target T and fixed fee F. Yes assigns
/// A + T - F to the author; No assigns T + A - F to contributors; Unknown returns
/// A - F to the author and T to contributors. Every audited branch assigns F to
/// the winning auditors. All actual A, including excess above the Policy minimum,
/// is at risk on No. T must be positive and F cannot exceed either side.
///
/// The caller additionally enforces F <= the Policy minimum bond at admission.
/// An author payout above `u64` returns [`DomainError::AmountOverflow`], while
/// contributor aggregates remain widened until individual claims are calculated.
pub fn audited_allocation(
    author: UsdcAmount,
    target: UsdcAmount,
    fee: UsdcAmount,
    outcome: AuditedOutcome,
) -> Result<FundedAllocation, DomainError> {
    require_target(target)?;
    if fee > author || fee > target {
        return Err(DomainError::InvalidAuditFee);
    }

    let total =
        LiabilityTotal::from_amount(author).checked_add(LiabilityTotal::from_amount(target))?;
    let (author_right, contributor_rights) = match outcome {
        AuditedOutcome::Yes => (
            author.checked_add(target.checked_sub(fee)?)?,
            LiabilityTotal::ZERO,
        ),
        AuditedOutcome::No => (
            UsdcAmount::ZERO,
            LiabilityTotal::from_amount(target)
                .checked_add(LiabilityTotal::from_amount(author.checked_sub(fee)?))?,
        ),
        AuditedOutcome::Unknown => (
            author.checked_sub(fee)?,
            LiabilityTotal::from_amount(target),
        ),
    };

    Ok(FundedAllocation {
        author: author_right,
        contributors: contributor_rights,
        audit_fee: fee,
        total,
    })
}

/// Return all actual principal on a separately authenticated technical timeout.
///
/// No audit fee is earned and no auditor answer is fabricated. The caller must
/// prove the applicable deadline, funded phase and absence of a timely auditor
/// quorum; the amount calculation alone cannot authorize timeout.
pub fn timeout_refunds(
    author: UsdcAmount,
    contributors: UsdcAmount,
) -> Result<FundedAllocation, DomainError> {
    Ok(FundedAllocation {
        author,
        contributors: LiabilityTotal::from_amount(contributors),
        audit_fee: UsdcAmount::ZERO,
        total: LiabilityTotal::from_amount(author)
            .checked_add(LiabilityTotal::from_amount(contributors))?,
    })
}

/// A direct audit consumes only its prepaid fee, regardless of the answer.
/// There is no bond, challenger reward, or separate refund beneficiary.
pub fn direct_audit_allocation(fee: UsdcAmount) -> FundedAllocation {
    FundedAllocation {
        author: UsdcAmount::ZERO,
        contributors: LiabilityTotal::ZERO,
        audit_fee: fee,
        total: LiabilityTotal::from_amount(fee),
    }
}

/// Calculate floor(R * principal / T), widening before multiplication.
///
/// R is the losing-author reward pool A - F. This is a reward share, excluding
/// returned contributor principal and any assigned rounding remainder. A zero
/// receipt principal is mathematically valid; it is not an admitted contribution.
pub fn reward_floor(
    reward_pool: UsdcAmount,
    principal: UsdcAmount,
    target: UsdcAmount,
) -> Result<UsdcAmount, DomainError> {
    require_target(target)?;
    if principal > target {
        return Err(DomainError::ChallengeAboveTarget);
    }

    let numerator = u128::from(reward_pool.base_units())
        .checked_mul(u128::from(principal.base_units()))
        .ok_or(DomainError::AmountOverflow)?;
    let floor = numerator / u128::from(target.base_units());

    u64::try_from(floor)
        .map(UsdcAmount::from_base_units)
        .map_err(|_| DomainError::AmountOverflow)
}

/// Return one contributor's No entitlement, giving the last claimant the remainder.
///
/// Earlier claimants receive principal plus floor(R * principal / T). The last
/// unclaimed receipt receives the entire remaining contributor liability, which
/// includes its principal, floor reward and any rounding remainder. Remaining
/// liability must exclude audit fees, discharged claims and unsolicited tokens.
///
/// The runtime authenticates positive receipt principal, the remaining liability
/// and whether this is the last unclaimed receipt. This calculation rejects
/// inconsistent bounds but cannot establish the absence of other claimants. Each
/// individual payout must fit `u64`, even when the aggregate allocation is wider.
pub fn no_contributor_claim(
    principal: UsdcAmount,
    reward_pool: UsdcAmount,
    target: UsdcAmount,
    remaining: LiabilityTotal,
    is_last: bool,
) -> Result<UsdcAmount, DomainError> {
    let reward = reward_floor(reward_pool, principal, target)?;
    if principal == UsdcAmount::ZERO {
        return Err(DomainError::InvalidRewardAllocation);
    }

    let ordinary =
        LiabilityTotal::from_amount(principal).checked_add(LiabilityTotal::from_amount(reward))?;
    let maximum = LiabilityTotal::from_amount(if is_last { principal } else { target })
        .checked_add(LiabilityTotal::from_amount(reward_pool))?;
    if remaining < ordinary || remaining > maximum || (!is_last && remaining == ordinary) {
        return Err(DomainError::InvalidRewardAllocation);
    }

    let payout = if is_last { remaining } else { ordinary };
    u64::try_from(payout.base_units())
        .map(UsdcAmount::from_base_units)
        .map_err(|_| DomainError::AmountOverflow)
}

fn require_target(target: UsdcAmount) -> Result<(), DomainError> {
    if target == UsdcAmount::ZERO {
        return Err(DomainError::ZeroChallengeTarget);
    }

    Ok(())
}
