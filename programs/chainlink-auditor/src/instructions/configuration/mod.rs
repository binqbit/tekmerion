pub mod register_profile;

pub use register_profile::{RegisterConfidentialProfile, RegisterConfidentialProfileBumps};

// Anchor resolves generated account modules through the crate root.
pub(crate) use register_profile::__client_accounts_register_confidential_profile;

#[cfg(feature = "cpi")]
pub(crate) use register_profile::__cpi_client_accounts_register_confidential_profile;
