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
    // Report IDs belong to the transport, not to audit stages. The signed payload
    // and Request lifecycle authenticate the execution, stage and replay boundary.
    require!(
        metadata.workflow_id == config.workflow_id,
        AdapterError::InvalidWorkflow
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_report_ids_do_not_replace_workflow_authentication() {
        let config = ConfidentialConfig {
            forwarder_program: Pubkey::new_unique(),
            forwarder_state: Pubkey::new_unique(),
            workflow_id: [7; 32],
            agent_config_hash: [8; 32],
            max_attempts: 3,
        };
        let mut lamports = 1;
        let mut data = [];
        let state = AccountInfo::new(
            &config.forwarder_state,
            false,
            false,
            &mut lamports,
            &mut data,
            &config.forwarder_program,
            false,
        );
        let mut metadata = [0; 64];
        metadata[..32].copy_from_slice(&config.workflow_id);
        // The CLI reuses 0001, whereas a deployed transport may assign other IDs.
        for report_id in [[0, 1], [0, 2], [0, 3], [9, 9]] {
            metadata[62..].copy_from_slice(&report_id);
            assert!(authenticate(&config, &state, authority(&config), &metadata).is_ok());
        }
        metadata[0] ^= 1;
        assert!(authenticate(&config, &state, authority(&config), &metadata).is_err());
        assert!(authenticate(&config, &state, Pubkey::new_unique(), &metadata).is_err());
    }
}
