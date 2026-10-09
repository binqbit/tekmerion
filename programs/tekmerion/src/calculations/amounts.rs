//! Checked base-unit arithmetic, independent of custody and account encoding.

use super::DomainError;

/// A nonnegative quantity of USDC base units used by pure calculations.
///
/// The internal `u64` is a computation representation, not an approved account
/// layout or serialized width. Constructing an amount does not establish actual
/// token credit, mint identity, or valid Policy limits.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct UsdcAmount(u64);

impl UsdcAmount {
    /// An amount containing no base units.
    pub const ZERO: Self = Self(0);

    /// Constructs an amount without selecting any Policy minimum or maximum.
    pub const fn from_base_units(base_units: u64) -> Self {
        Self(base_units)
    }

    /// Returns the integer base-unit quantity.
    pub const fn base_units(self) -> u64 {
        self.0
    }

    /// Adds two amounts without wrapping.
    ///
    /// Returns [`DomainError::AmountOverflow`] if the internal representation
    /// cannot hold their sum.
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::AmountOverflow)
    }

    /// Subtracts an amount without allowing a negative result.
    ///
    /// Returns [`DomainError::AmountUnderflow`] when `other` exceeds this amount.
    pub fn checked_sub(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(DomainError::AmountUnderflow)
    }
}

/// A widened total for checking disjoint base-unit liabilities.
///
/// The internal `u128` is a computation representation only, not an approved
/// account layout or serialized width. No narrowing conversion is provided;
/// a total does not itself establish custody backing or a right to payment.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LiabilityTotal(u128);

impl LiabilityTotal {
    /// A total containing no base units.
    pub const ZERO: Self = Self(0);

    /// Widens a base-unit amount without losing precision.
    pub const fn from_amount(amount: UsdcAmount) -> Self {
        Self(amount.base_units() as u128)
    }

    /// Returns the widened integer base-unit total.
    pub const fn base_units(self) -> u128 {
        self.0
    }

    /// Adds two totals without wrapping or narrowing.
    ///
    /// Returns [`DomainError::AmountOverflow`] if the internal representation
    /// cannot hold their sum.
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::AmountOverflow)
    }
}
