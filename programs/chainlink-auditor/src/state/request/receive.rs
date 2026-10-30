use anchor_lang::prelude::*;
use tekmerion_sdk::program::Case;

use super::{ConfidentialRequest, ConfidentialState};
use crate::{
    binding::require_open,
    confidential::{
        VERSION,
        payload::{AuditReport, ReportBody, ReportPrefix, retryable},
    },
    errors::AdapterError,
    state::ConfidentialProfile,
};

pub struct ReceiveContext<'a> {
    pub profile: &'a ConfidentialProfile,
    pub case: &'a Case,
    pub now: i64,
}

impl ConfidentialRequest {
    /// Called only after native Forwarder and immutable task authentication.
    pub fn receive(&mut self, report: &AuditReport, context: ReceiveContext<'_>) -> Result<()> {
        let prefix = &report.prefix;
        require!(
            self.version == VERSION && prefix.request_digest == self.digest,
            AdapterError::InvalidBinding
        );
        let body_hash = report.digest()?;
        let current_index = self.attempts_started.checked_sub(1);

        if current_index == Some(prefix.attempt_index) {
            require!(
                self.execution_id == prefix.execution_id,
                AdapterError::ConflictingReport
            );
            if matches!(report.body, ReportBody::InputAccepted) {
                return Ok(());
            }
            if self.state != ConfidentialState::Active {
                require!(
                    self.terminal_payload_hash == body_hash,
                    AdapterError::ConflictingReport
                );
                return Ok(());
            }
        } else {
            // A later execution needs an authenticated failure; old indexes never return.
            require!(
                matches!(report.body, ReportBody::InputAccepted)
                    && prefix.attempt_index == self.attempts_started,
                AdapterError::InvalidAttempt
            );
        }

        require_open(context.case, context.now)?;
        match &report.body {
            ReportBody::InputAccepted => self.claim_attempt(prefix, context),
            _ => self.finish_attempt(report, body_hash, context),
        }
    }

    fn claim_attempt(&mut self, prefix: &ReportPrefix, context: ReceiveContext<'_>) -> Result<()> {
        self.can_start(context.profile, context.case, context.now)?;
        require!(
            prefix.attempt_index == self.attempts_started,
            AdapterError::InvalidAttempt
        );
        self.attempts_started = self
            .attempts_started
            .checked_add(1)
            .ok_or(AdapterError::AttemptsExhausted)?;
        self.execution_id = prefix.execution_id;
        self.claimed_at = context.now;
        self.terminal_at = 0;
        self.terminal_payload_hash = [0; 32];
        self.failure_code = 0;
        self.report_hash = [0; 32];
        self.state = ConfidentialState::Active;
        Ok(())
    }

    fn finish_attempt(
        &mut self,
        report: &AuditReport,
        body_hash: [u8; 32],
        context: ReceiveContext<'_>,
    ) -> Result<()> {
        require!(
            self.state == ConfidentialState::Active
                && self.attempts_started.checked_sub(1) == Some(report.prefix.attempt_index)
                && self.execution_id == report.prefix.execution_id,
            AdapterError::InvalidAttempt
        );
        match &report.body {
            ReportBody::Verdict {
                report_hash,
                answer,
                reason,
            } => {
                self.report_hash = *report_hash;
                self.answer = *answer;
                self.reason = *reason;
                self.state = ConfidentialState::Ready;
            }
            ReportBody::AttemptFailed { code, report_hash } => {
                self.failure_code = *code;
                self.report_hash = *report_hash;
                self.state = if retryable(*code)?
                    && self.attempts_started < context.profile.config.max_attempts
                {
                    ConfidentialState::Retryable
                } else {
                    ConfidentialState::Stopped
                };
            }
            ReportBody::InputAccepted => return err!(AdapterError::InvalidPayload),
        }
        self.terminal_at = context.now;
        self.terminal_payload_hash = body_hash;
        Ok(())
    }
}
