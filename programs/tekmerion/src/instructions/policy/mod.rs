pub mod acquire_policy;
pub mod create_policy;
pub mod update_policy;

pub use acquire_policy::{AcquirePolicy, AcquirePolicyBumps};
pub use create_policy::{CreatePolicy, CreatePolicyBumps};
pub use update_policy::{UpdatePolicy, UpdatePolicyBumps};

// Anchor resolves generated client account modules from the crate root.
pub(crate) use acquire_policy::__client_accounts_acquire_policy;
pub(crate) use create_policy::__client_accounts_create_policy;
pub(crate) use update_policy::__client_accounts_update_policy;

#[cfg(feature = "cpi")]
pub(crate) use acquire_policy::__cpi_client_accounts_acquire_policy;
#[cfg(feature = "cpi")]
pub(crate) use create_policy::__cpi_client_accounts_create_policy;
#[cfg(feature = "cpi")]
pub(crate) use update_policy::__cpi_client_accounts_update_policy;
