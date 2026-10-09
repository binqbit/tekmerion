//! Builders never sign or submit. The `cpi` feature exposes the program's generated
//! Anchor CPI API. The default `host` feature adds account views;
//! disable it for onchain consumers. Account views validate supplied
//! observations but cannot authenticate an RPC server or establish chain finality.

#[cfg(feature = "host")]
pub mod accounts;
#[cfg(feature = "host")]
mod error;
pub mod instructions;

pub use anchor_lang::prelude::Pubkey;
#[cfg(feature = "host")]
pub use error::{Result, SdkError};
pub use solana_instruction::Instruction;
pub use tekmerion as program;
#[cfg(feature = "cpi")]
pub use tekmerion::cpi;
pub use tekmerion::{ID, TOKEN_DECIMALS, USDC_MINT};
