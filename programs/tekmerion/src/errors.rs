use anchor_lang::prelude::*;

use crate::calculations::DomainError;

pub(crate) fn domain_error(error: DomainError) -> Error {
    match error {
        DomainError::FundingClosed => error!(TekmerionError::FundingClosed),
        DomainError::FundingStillOpen => error!(TekmerionError::FundingStillOpen),
        DomainError::ChallengeAlreadyFunded => error!(TekmerionError::AlreadyFunded),
        DomainError::AmountOverflow
        | DomainError::AmountUnderflow
        | DomainError::TimeOverflow
        | DomainError::ChallengeAboveTarget
        | DomainError::ZeroChallengeTarget
        | DomainError::InvalidAuditFee
        | DomainError::InvalidRewardAllocation => error!(TekmerionError::ArithmeticOverflow),
    }
}

#[error_code]
pub enum TekmerionError {
    #[msg("Invalid Policy rules")]
    InvalidRules,
    #[msg("Checked arithmetic failed")]
    ArithmeticOverflow,
    #[msg("Policy has a retained dependency")]
    PolicyInUse,
    #[msg("Subject must be nonexecutable; unexpected accounts are not allowed")]
    InvalidSubject,
    #[msg("Attempt does not match the canonical preceding Case")]
    InvalidHistory,
    #[msg("Actual author principal is below the required bond")]
    InsufficientBond,
    #[msg("Unsupported or invalid token account")]
    InvalidTokenAccount,
    #[msg("Actual token movement differs from the authorized amount")]
    InvalidTransfer,
    #[msg("Operation is unavailable in the current phase")]
    InvalidPhase,
    #[msg("Funding deadline has elapsed")]
    FundingClosed,
    #[msg("Funding deadline has not elapsed")]
    FundingStillOpen,
    #[msg("Challenge target is already reached")]
    AlreadyFunded,
    #[msg("Contribution must be positive")]
    InvalidContribution,
    #[msg("The author cannot contribute to their own challenge")]
    UnsupportedRole,
    #[msg("Receipt does not match this Case and participant")]
    InvalidReceipt,
    #[msg("Custody does not cover every outstanding entitlement")]
    InsufficientBacking,
    #[msg("Entitlement has already been claimed")]
    AlreadyClaimed,
    #[msg("Signer is not a configured auditor")]
    UnauthorizedAuditor,
    #[msg("Auditor has already submitted a vote")]
    DuplicateAuditVote,
    #[msg("Fee destinations do not match the winning quorum")]
    InvalidPayoutAccounts,
    #[msg("Audit answer and reason are incompatible")]
    InvalidAuditResponse,
    #[msg("Audit deadline has elapsed")]
    AuditExpired,
    #[msg("Audit deadline has not elapsed")]
    AuditStillOpen,
    #[msg("Fee destination is not owned by the configured beneficiary")]
    InvalidFeeRecipient,
}
