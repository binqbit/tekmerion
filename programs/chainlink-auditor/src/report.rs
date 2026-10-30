//! Authenticate the delivery envelope before interpreting a verdict.

use crate::{errors::AdapterError, payload::WorkflowMetadata, state::ConfidentialConfig};
use anchor_lang::prelude::*;

/// The Forwarder's signer is scoped to its state and this receiver program.
pub fn authority(config: &ConfidentialConfig) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"forwarder",
            config.forwarder_state.as_ref(),
            crate::ID.as_ref(),
        ],
        &config.forwarder_program,
    )
    .0
}

pub(crate) fn authenticate(
    config: &ConfidentialConfig,
    stage: u8,
    forwarder_state: &AccountInfo<'_>,
    forwarder_authority: Pubkey,
    metadata: &[u8],
) -> Result<()> {
    require_keys_eq!(
        forwarder_state.key(),
        config.forwarder_state,
        AdapterError::InvalidForwarder
    );
    require_keys_eq!(
        *forwarder_state.owner,
        config.forwarder_program,
        AdapterError::InvalidForwarder
    );
    // Anchor requires this authority to sign the receiver instruction.
    require_keys_eq!(
        forwarder_authority,
        authority(config),
        AdapterError::InvalidForwarder
    );

    let metadata = WorkflowMetadata::decode(metadata)?;
    require!(
        metadata.workflow_id == config.workflow_id
            && metadata.report_id == crate::confidential::payload::report_id(stage)?,
        AdapterError::InvalidWorkflow
    );
    Ok(())
}
