use anchor_lang::{AccountDeserialize, prelude::Pubkey};

use crate::{Result, SdkError};

/// One caller-supplied bank observation, including its chain Clock value.
/// The transport must establish common provenance; a slot label is not proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observation {
    pub slot: u64,
    pub unix_timestamp: i64,
}

/// Borrowed account bytes from the observation slot, never an indexed projection.
#[derive(Clone, Copy, Debug)]
pub struct AccountData<'a> {
    pub key: Pubkey,
    pub owner: Pubkey,
    pub executable: bool,
    pub data: &'a [u8],
    pub slot: u64,
}

impl AccountData<'_> {
    pub(super) fn check(&self, observation: Observation, owner: Pubkey) -> Result<()> {
        if self.slot != observation.slot {
            return Err(SdkError::InconsistentObservation);
        }
        if self.owner != owner || self.executable {
            return Err(SdkError::InvalidAccount);
        }
        Ok(())
    }

    pub(crate) fn decode<T: AccountDeserialize>(
        &self,
        observation: Observation,
        owner: Pubkey,
        size: usize,
    ) -> Result<T> {
        self.check(observation, owner)?;
        if self.data.len() != size {
            return Err(SdkError::InvalidAccount);
        }
        let mut data = self.data;
        T::try_deserialize(&mut data).map_err(|_| SdkError::InvalidAccount)
    }
}
