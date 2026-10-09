//! Pure funding, deadline, terminal allocation and contributor reward calculations.
//!
//! Callers authenticate state, chain time, custody and operation phase at the
//! mutation boundary. A quote neither transfers funds nor creates rights.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod amounts;
pub mod errors;
pub mod funding;
pub mod settlement;
pub mod time;

pub use errors::DomainError;
