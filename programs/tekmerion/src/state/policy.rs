use anchor_lang::prelude::*;

use crate::calculations::time::{ChainDeadline, ChainTimestamp};
use crate::errors::TekmerionError;

/// Bounded committee size keeps account storage and settlement work predictable.
pub const MAX_AUDITORS: usize = 9;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct AuditorConfig {
    /// Wallet signer or a PDA that signs through its owning program.
    pub authority: Pubkey,
    /// Wallet owning the USDC account that receives this auditor's fee share.
    pub fee_recipient: Pubkey,
}

impl AuditorConfig {
    pub const SPACE: usize = 2 * 32;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PolicyRules {
    // Committee and voting threshold.
    pub auditors: Vec<AuditorConfig>,
    pub threshold: u8,

    // Collateral and total audit fee.
    pub author_bond: u64,
    pub challenge_target: u64,
    pub audit_fee: u64,

    // Funding and audit windows.
    pub funding_duration: u64,
    pub audit_timeout: u64,
}

impl PolicyRules {
    pub const SPACE: usize = 4 + MAX_AUDITORS * AuditorConfig::SPACE + 1 + 5 * 8;

    /// Strict majority prevents two different answers from reaching quorum.
    pub fn has_valid_parameters(&self) -> bool {
        let count = self.auditors.len();
        let threshold = usize::from(self.threshold);
        if !(1..=MAX_AUDITORS).contains(&count) || threshold <= count / 2 || threshold > count {
            return false;
        }

        for (index, auditor) in self.auditors.iter().enumerate() {
            if auditor.authority == Pubkey::default() || auditor.fee_recipient == Pubkey::default()
            {
                return false;
            }

            for previous in &self.auditors[..index] {
                if previous.authority == auditor.authority
                    || previous.fee_recipient == auditor.fee_recipient
                {
                    return false;
                }
            }
        }

        if self.author_bond == 0 || self.challenge_target == 0 {
            return false;
        }

        if self.funding_duration == 0 || self.audit_timeout == 0 {
            return false;
        }

        self.audit_fee <= self.author_bond && self.audit_fee <= self.challenge_target
    }

    /// Check static rules and both deadline calculations against trusted chain time.
    pub fn validate_at(&self, now: i64) -> Result<()> {
        require!(self.has_valid_parameters(), TekmerionError::InvalidRules);

        let now = ChainTimestamp::from_unix_seconds(now);
        ChainDeadline::checked_after(now, self.funding_duration)
            .map_err(|_| error!(TekmerionError::ArithmeticOverflow))?;
        ChainDeadline::checked_after(now, self.audit_timeout)
            .map_err(|_| error!(TekmerionError::ArithmeticOverflow))?;

        Ok(())
    }
}

#[account]
#[derive(Debug)]
pub struct Policy {
    // Identity and authorization.
    pub admin: Pubkey,
    pub identifier: u64,
    /// Admin-selected signer identity; its key or PDA derivation belongs to the client.
    pub client_authority: Pubkey,

    // Configuration.
    pub rules: PolicyRules,

    // Retained dependencies.
    // Uses are retained permanently, including after a Case is finalized.
    // Any use locks the rules so existing Cases and client dependencies stay valid.
    pub active_uses: u64,

    // PDA metadata.
    pub bump: u8,
}

impl Policy {
    pub const SPACE: usize = 8 + 32 + 8 + 32 + PolicyRules::SPACE + 8 + 1;
    pub const SEED: &'static [u8] = b"policy";

    pub fn address(admin: &Pubkey, identifier: u64) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[Self::SEED, admin.as_ref(), &identifier.to_le_bytes()],
            &crate::ID,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(count: usize, threshold: u8) -> PolicyRules {
        PolicyRules {
            auditors: (0..count)
                .map(|_| AuditorConfig {
                    authority: Pubkey::new_unique(),
                    fee_recipient: Pubkey::new_unique(),
                })
                .collect(),
            threshold,
            author_bond: 125,
            challenge_target: 100,
            audit_fee: 7,
            funding_duration: 60,
            audit_timeout: 120,
        }
    }

    #[test]
    fn committee_requires_nonempty_bounded_strict_majority() {
        for (count, threshold) in [(1, 1), (3, 2), (5, 3), (9, 5), (9, 9)] {
            assert!(rules(count, threshold).has_valid_parameters());
        }
        for (count, threshold) in [(0, 0), (3, 0), (3, 1), (3, 4), (4, 2), (10, 6)] {
            assert!(!rules(count, threshold).has_valid_parameters());
        }
    }

    #[test]
    fn committee_rejects_duplicate_and_zero_identities() {
        let mut candidate = rules(3, 2);
        candidate.auditors[1].authority = candidate.auditors[0].authority;
        assert!(!candidate.has_valid_parameters());
        let mut candidate = rules(3, 2);
        candidate.auditors[1].fee_recipient = candidate.auditors[0].fee_recipient;
        assert!(!candidate.has_valid_parameters());
        let mut candidate = rules(3, 2);
        candidate.auditors[0].authority = Pubkey::default();
        assert!(!candidate.has_valid_parameters());
        let mut candidate = rules(3, 2);
        candidate.auditors[0].fee_recipient = Pubkey::default();
        assert!(!candidate.has_valid_parameters());
    }

    #[test]
    fn maximum_policy_serialization_fits_reserved_space() {
        let policy = Policy {
            admin: Pubkey::new_unique(),
            identifier: 0,
            client_authority: Pubkey::new_unique(),
            rules: rules(MAX_AUDITORS, 5),
            active_uses: 0,
            bump: 255,
        };
        let mut serialized = Vec::new();
        policy.try_serialize(&mut serialized).unwrap();
        assert_eq!(serialized.len(), Policy::SPACE);
    }
}
