use std::{error::Error, fmt};

pub type Result<T> = std::result::Result<T, SdkError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SdkError {
    InconsistentObservation,
    InvalidAccount,
    InvalidRelationship,
    InvalidState,
    InsufficientBacking,
    ActionUnavailable,
    Arithmetic,
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InconsistentObservation => "accounts must come from the same observation slot",
            Self::InvalidAccount => "invalid account owner, executable flag, size or encoding",
            Self::InvalidRelationship => {
                "account identity or relationship does not match the expected account"
            }
            Self::InvalidState => "account state is inconsistent with the supported lifecycle",
            Self::InsufficientBacking => "vault balance does not cover unpaid rights",
            Self::ActionUnavailable => "the requested action is unavailable in this observation",
            Self::Arithmetic => "amount calculation failed",
        })
    }
}

impl Error for SdkError {}
