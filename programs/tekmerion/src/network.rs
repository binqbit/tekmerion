//! USDC identities selected when compiling the program and its clients.
//! Mainnet is the default; devnet requires `--no-default-features --features devnet`.

#[cfg(all(feature = "mainnet", feature = "devnet"))]
compile_error!(
    "Select exactly one network: mainnet or devnet; devnet requires --no-default-features"
);

#[cfg(not(any(feature = "mainnet", feature = "devnet")))]
compile_error!("Select exactly one network: mainnet or devnet");

pub use selected::{NAME, USDC_MINT};

#[cfg(not(feature = "devnet"))]
mod selected {
    use anchor_lang::prelude::*;

    pub const NAME: &str = "mainnet";
    pub const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
}

#[cfg(feature = "devnet")]
mod selected {
    use anchor_lang::prelude::*;

    pub const NAME: &str = "devnet";
    pub const USDC_MINT: Pubkey = pubkey!("4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU");
}
