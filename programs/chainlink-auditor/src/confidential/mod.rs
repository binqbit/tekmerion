//! Confidential workflow execution with authenticated, case-bound results.

pub mod binding;
pub mod payload;
pub mod release;

pub const VERSION: u8 = 3;
pub const DEFAULT_MAX_ATTEMPTS: u16 = 3;

pub fn serialize(value: &impl anchor_lang::AnchorSerialize) -> anchor_lang::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    value.serialize(&mut bytes)?;
    Ok(bytes)
}
