use std::{error::Error, fmt};

use solana_rpc_client::api::client_error::{Error as RpcError, SignerError, TransactionError};

use crate::{Pubkey, SdkError};

pub type Result<T> = std::result::Result<T, ClientError>;

#[derive(Debug)]
pub enum ClientError {
    Rpc(RpcError),
    Validation(SdkError),
    MissingAccount(Pubkey),
    MalformedResponse(&'static str),
    Signing(SignerError),
    InvalidSignature(TransactionError),
    InvalidTransaction(&'static str),
    InvalidPollInterval,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(error) => write!(f, "RPC error: {error}"),
            Self::Validation(error) => write!(f, "account validation failed: {error}"),
            Self::MissingAccount(key) => write!(f, "account not found: {key}"),
            Self::MalformedResponse(message) => write!(f, "invalid RPC response: {message}"),
            Self::Signing(error) => write!(f, "signing failed: {error}"),
            Self::InvalidSignature(error) => write!(f, "invalid transaction signature: {error}"),
            Self::InvalidTransaction(message) => write!(f, "invalid transaction: {message}"),
            Self::InvalidPollInterval => f.write_str("polling interval must be positive"),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Rpc(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::Signing(error) => Some(error),
            Self::InvalidSignature(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RpcError> for ClientError {
    fn from(error: RpcError) -> Self {
        Self::Rpc(error)
    }
}

impl From<SdkError> for ClientError {
    fn from(error: SdkError) -> Self {
        Self::Validation(error)
    }
}

impl From<SignerError> for ClientError {
    fn from(error: SignerError) -> Self {
        Self::Signing(error)
    }
}
