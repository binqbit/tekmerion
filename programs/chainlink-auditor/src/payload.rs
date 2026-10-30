use crate::errors::AdapterError;
use anchor_lang::prelude::*;
use tekmerion_sdk::program::{AuditAnswer, AuditDecision, AuditReason};

pub const METADATA_LEN: usize = 64;

/// Fixed Forwarder metadata order: workflow ID, name, owner, report ID.
pub(crate) struct WorkflowMetadata {
    pub workflow_id: [u8; 32],
    pub report_id: [u8; 2],
}

impl WorkflowMetadata {
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        require!(bytes.len() == METADATA_LEN, AdapterError::InvalidWorkflow);
        let mut workflow_id = [0; 32];
        workflow_id.copy_from_slice(&bytes[..32]);
        Ok(Self {
            workflow_id,
            report_id: [bytes[METADATA_LEN - 2], bytes[METADATA_LEN - 1]],
        })
    }
}

pub fn decision(answer: u8, reason: u8) -> Result<AuditDecision> {
    let answer = match answer {
        0 => AuditAnswer::Yes,
        1 => AuditAnswer::No,
        2 => AuditAnswer::Unknown,
        _ => return err!(AdapterError::InvalidPayload),
    };
    let reason = match reason {
        0 => AuditReason::Decisive,
        1 => AuditReason::MissingEvidence,
        2 => AuditReason::StaleEvidence,
        3 => AuditReason::ConflictingEvidence,
        4 => AuditReason::UnsupportedEvidence,
        5 => AuditReason::Inconclusive,
        _ => return err!(AdapterError::InvalidPayload),
    };
    require!(answer.accepts_reason(reason), AdapterError::InvalidPayload);
    Ok(AuditDecision { answer, reason })
}
