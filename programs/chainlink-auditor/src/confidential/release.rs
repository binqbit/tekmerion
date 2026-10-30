//! Enrollment is a release decision, not an admin assertion about arbitrary code.
use crate::{errors::AdapterError, state::ConfidentialConfig};
use anchor_lang::prelude::*;

pub fn validate(config: &ConfidentialConfig) -> Result<()> {
    // No live tenant/deployment has been qualified. Never invent public IDs.
    #[cfg(not(feature = "local-fixtures"))]
    {
        let _ = config;
        err!(AdapterError::UnqualifiedRelease)
    }
    #[cfg(feature = "local-fixtures")]
    {
        require!(
            config.forwarder_program == pubkey!("whV7Q5pi17hPPyaPksToDw1nMx6Lh8qmNWKFaLRQ4wz")
                && config.forwarder_state
                    == pubkey!("Frh1NMSHLraTyJMzbNeQf3YELAYe3DmfmHqvpVeHi7vf")
                && config.workflow_id == [0x11; 32]
                && config.agent_config_hash != [0; 32],
            AdapterError::UnqualifiedRelease
        );
        Ok(())
    }
}

#[cfg(all(test, not(feature = "local-fixtures")))]
mod tests {
    use super::*;

    #[test]
    fn production_cannot_enroll_an_unqualified_confidential_workflow() {
        let config = ConfidentialConfig {
            forwarder_program: pubkey!("whV7Q5pi17hPPyaPksToDw1nMx6Lh8qmNWKFaLRQ4wz"),
            forwarder_state: pubkey!("Frh1NMSHLraTyJMzbNeQf3YELAYe3DmfmHqvpVeHi7vf"),
            workflow_id: [0x11; 32],
            agent_config_hash: [0x44; 32],
            max_attempts: 3,
        };
        assert!(
            validate(&config)
                .unwrap_err()
                .to_string()
                .contains("UnqualifiedRelease")
        );
    }
}
