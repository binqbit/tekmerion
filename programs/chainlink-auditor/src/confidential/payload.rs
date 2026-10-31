use crate::{errors::AdapterError, payload::decision};
use anchor_lang::prelude::*;
use solana_sha256_hasher::hash;

pub const PREFIX_LEN: usize = 68;
pub const VERDICT_LEN: usize = 102;
pub const FAILURE_LEN: usize = 101;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ReportPrefix {
    pub version: u8,
    pub stage: u8,
    pub request_digest: [u8; 32],
    pub attempt_index: u16,
    pub execution_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportBody {
    InputAccepted,
    Verdict {
        report_hash: [u8; 32],
        answer: u8,
        reason: u8,
    },
    AttemptFailed {
        code: u8,
        report_hash: [u8; 32],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditReport {
    pub prefix: ReportPrefix,
    pub body: ReportBody,
}

pub fn retryable(code: u8) -> Result<bool> {
    match code {
        1..=4 => Ok(true),
        64..=70 => Ok(false),
        _ => err!(AdapterError::InvalidPayload),
    }
}

impl AuditReport {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        require!(bytes.len() >= PREFIX_LEN, AdapterError::InvalidPayload);
        let prefix = ReportPrefix::try_from_slice(&bytes[..PREFIX_LEN])
            .map_err(|_| error!(AdapterError::InvalidPayload))?;
        require!(
            prefix.version == super::VERSION
                && [prefix.request_digest, prefix.execution_id]
                    .iter()
                    .all(|v| *v != [0; 32]),
            AdapterError::InvalidPayload
        );
        let tail = &bytes[PREFIX_LEN..];
        let body = match (prefix.stage, bytes.len()) {
            (0, PREFIX_LEN) => ReportBody::InputAccepted,
            (1, VERDICT_LEN) => {
                let report_hash = tail[..32].try_into().unwrap();
                require!(report_hash != [0; 32], AdapterError::InvalidPayload);
                decision(tail[32], tail[33])?;
                ReportBody::Verdict {
                    report_hash,
                    answer: tail[32],
                    reason: tail[33],
                }
            }
            (2, FAILURE_LEN) => {
                retryable(tail[0])?;
                let report_hash = tail[1..33].try_into().unwrap();
                require!(report_hash != [0; 32], AdapterError::InvalidPayload);
                ReportBody::AttemptFailed {
                    code: tail[0],
                    report_hash,
                }
            }
            _ => return err!(AdapterError::InvalidPayload),
        };
        Ok(Self { prefix, body })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = super::serialize(&self.prefix)?;
        match &self.body {
            ReportBody::InputAccepted => {}
            ReportBody::Verdict {
                report_hash,
                answer,
                reason,
            } => {
                bytes.extend(report_hash);
                bytes.push(*answer);
                bytes.push(*reason);
            }
            ReportBody::AttemptFailed { code, report_hash } => {
                bytes.push(*code);
                bytes.extend(report_hash);
            }
        }
        Self::decode(&bytes)?;
        Ok(bytes)
    }

    pub fn digest(&self) -> Result<[u8; 32]> {
        Ok(hash(&self.encode()?).to_bytes())
    }
}
