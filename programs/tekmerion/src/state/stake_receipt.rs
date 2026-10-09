use anchor_lang::prelude::*;

use crate::errors::TekmerionError;

pub const AUTHOR_SIDE: u8 = 0;
pub const CHALLENGE_SIDE: u8 = 1;

#[account]
#[derive(Debug)]
pub struct StakeReceipt {
    // Case and participant identity.
    pub case: Pubkey,
    pub owner: Pubkey,
    pub side: u8,

    // Accepted principal and payment progress.
    pub principal: u64,
    pub claimed: bool,

    // PDA metadata.
    pub bump: u8,
}

impl StakeReceipt {
    pub const SPACE: usize = 8 + 32 + 32 + 1 + 8 + 1 + 1;
    pub const SEED: &'static [u8] = b"receipt";

    pub fn address(case: &Pubkey, owner: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, case.as_ref(), owner.as_ref()], &crate::ID)
    }

    /// Created once with the Case after the exact author bond transfer succeeds.
    pub(crate) fn new_author(case: Pubkey, owner: Pubkey, principal: u64, bump: u8) -> Self {
        Self {
            case,
            owner,
            side: AUTHOR_SIDE,
            principal,
            claimed: false,
            bump,
        }
    }

    /// The caller authenticates the canonical Case/author PDA and its bindings.
    pub(crate) fn require_author_principal(&self, principal: u64) -> Result<()> {
        self.require_side(AUTHOR_SIDE)?;
        require!(self.principal == principal, TekmerionError::InvalidReceipt);

        Ok(())
    }

    /// Prepare the accepted top-up without recording it before custody succeeds.
    pub(crate) fn prepare_contribution(
        &self,
        case: Pubkey,
        owner: Pubkey,
        bump: u8,
        accepted: u64,
    ) -> Result<Self> {
        if self.case == Pubkey::default() {
            require!(
                self.owner == Pubkey::default()
                    && self.side == 0
                    && self.principal == 0
                    && !self.claimed
                    && self.bump == 0,
                TekmerionError::InvalidReceipt
            );
        } else {
            require_keys_eq!(self.case, case, TekmerionError::InvalidReceipt);
            require_keys_eq!(self.owner, owner, TekmerionError::InvalidReceipt);
            require!(
                self.side == CHALLENGE_SIDE
                    && self.principal > 0
                    && !self.claimed
                    && self.bump == bump,
                TekmerionError::InvalidReceipt
            );
        }

        let principal = self
            .principal
            .checked_add(accepted)
            .ok_or(TekmerionError::ArithmeticOverflow)?;

        Ok(Self {
            case,
            owner,
            side: CHALLENGE_SIDE,
            principal,
            claimed: false,
            bump,
        })
    }

    pub(crate) fn require_claimable(&self, side: u8) -> Result<()> {
        self.require_side(side)?;
        require!(!self.claimed, TekmerionError::AlreadyClaimed);

        Ok(())
    }

    fn require_side(&self, side: u8) -> Result<()> {
        require!(
            matches!(side, AUTHOR_SIDE | CHALLENGE_SIDE) && self.side == side && self.principal > 0,
            TekmerionError::InvalidReceipt
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn author_receipt() -> StakeReceipt {
        StakeReceipt::new_author(Pubkey::new_unique(), Pubkey::new_unique(), 125, 255)
    }

    #[test]
    fn author_receipt_cannot_be_topped_up_or_claimed_as_a_challenge() {
        let receipt = author_receipt();
        assert!(receipt.require_author_principal(125).is_ok());
        assert!(receipt.require_author_principal(124).is_err());
        assert!(receipt.require_claimable(CHALLENGE_SIDE).is_err());
        assert!(
            receipt
                .prepare_contribution(receipt.case, receipt.owner, receipt.bump, 1)
                .is_err()
        );
        assert_eq!(receipt.principal, 125);
        assert_eq!(receipt.side, AUTHOR_SIDE);
    }

    #[test]
    fn author_claim_status_remains_readable_after_discharge_but_cannot_replay() {
        let mut receipt = author_receipt();
        assert!(receipt.require_claimable(AUTHOR_SIDE).is_ok());
        receipt.claimed = true;
        assert!(receipt.require_author_principal(125).is_ok());
        assert!(receipt.require_claimable(AUTHOR_SIDE).is_err());

        receipt.claimed = false;
        receipt.side = CHALLENGE_SIDE;
        assert!(receipt.require_author_principal(125).is_err());
        assert!(receipt.require_claimable(AUTHOR_SIDE).is_err());
        assert!(receipt.require_claimable(CHALLENGE_SIDE).is_ok());
    }

    #[test]
    fn either_receipt_role_fits_reserved_space() {
        for side in [AUTHOR_SIDE, CHALLENGE_SIDE] {
            let mut receipt = author_receipt();
            receipt.side = side;
            let mut serialized = Vec::new();
            receipt.try_serialize(&mut serialized).unwrap();
            assert_eq!(serialized.len(), StakeReceipt::SPACE);
        }
    }
}
