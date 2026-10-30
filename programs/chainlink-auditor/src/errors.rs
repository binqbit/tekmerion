use anchor_lang::prelude::*;

#[error_code]
pub enum AdapterError {
    #[msg("Invalid or unsupported profile configuration")]
    InvalidProfile,
    #[msg("Policy or Case identity does not match the profile")]
    InvalidBinding,
    #[msg("Case is not open for an audit")]
    AuditClosed,
    #[msg("Only the pinned forwarder authority may deliver reports")]
    InvalidForwarder,
    #[msg("Report metadata does not match the pinned workflow")]
    InvalidWorkflow,
    #[msg("Invalid report version, length, commitment or decision")]
    InvalidPayload,
    #[msg("A different verdict is already recorded")]
    ConflictingReport,
    #[msg("No authenticated verdict is ready to apply")]
    NotReady,
    #[msg("Invalid immutable workflow configuration")]
    InvalidConfiguration,
    #[msg("This confidential release has not been qualified for enrollment")]
    UnqualifiedRelease,
    #[msg("The attempt is not authorized by the current request state")]
    InvalidAttempt,
    #[msg("The configured attempt limit or execution deadline is exhausted")]
    AttemptsExhausted,
}
