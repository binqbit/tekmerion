//! Checked chain-clock calculations for funding and audit deadlines.

use super::DomainError;

/// A chain-clock observation used by pure calculations.
///
/// The internal signed seconds representation is not a serialized account
/// layout. The runtime adapter must authenticate the trusted chain Clock;
/// constructing this value does not authenticate a caller-provided timestamp.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChainTimestamp(i64);

impl ChainTimestamp {
    /// Constructs an observation from signed Unix seconds.
    pub const fn from_unix_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    /// Returns the signed Unix seconds of the observation.
    pub const fn unix_seconds(self) -> i64 {
        self.0
    }
}

/// An explicit deadline used by pure calculations.
///
/// The window is open strictly before this boundary and elapsed at or after it.
/// The internal signed seconds representation does not define a serialized
/// account layout and does not select an audit timeout origin or cutoff.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChainDeadline(i64);

impl ChainDeadline {
    /// Constructs a boundary from signed Unix seconds.
    pub const fn from_unix_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    /// Returns the signed Unix seconds of the boundary.
    pub const fn unix_seconds(self) -> i64 {
        self.0
    }

    /// Returns whether the observation is strictly before the deadline.
    pub const fn is_open_at(self, now: ChainTimestamp) -> bool {
        now.0 < self.0
    }

    /// Returns whether the observation is at or after the deadline.
    pub const fn is_elapsed_at(self, now: ChainTimestamp) -> bool {
        now.0 >= self.0
    }

    /// Calculates a boundary after an observation using checked seconds.
    ///
    /// Zero seconds is mathematically valid here; permitted Policy durations
    /// remain a separate validation concern. The calculation widens both inputs
    /// before addition, so a duration need not itself fit into signed seconds.
    /// Returns [`DomainError::TimeOverflow`] if the resulting timestamp cannot
    /// be represented as signed Unix seconds.
    pub fn checked_after(now: ChainTimestamp, seconds: u64) -> Result<Self, DomainError> {
        let deadline = i128::from(now.0)
            .checked_add(i128::from(seconds))
            .ok_or(DomainError::TimeOverflow)?;

        i64::try_from(deadline)
            .map(Self)
            .map_err(|_| DomainError::TimeOverflow)
    }
}
