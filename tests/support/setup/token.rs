use solana_account::AccountSharedData;
use solana_address::Address;
use solana_keypair::Keypair;
use solana_program_pack::Pack;
use solana_program_test::{BanksClientError, ProgramTestContext};
use solana_signer::Signer;
use spl_token_interface::{
    instruction,
    state::{Account, Mint},
};

use super::context::send;

/// Initialize a mint with genuine System and SPL Token instructions.
pub async fn create_mint(
    context: &mut ProgramTestContext,
    mint: &Keypair,
    authority: Address,
    freeze_authority: Option<Address>,
    decimals: u8,
) -> Result<(), BanksClientError> {
    let rent = context.banks_client.get_rent().await?;
    let create = solana_system_interface::instruction::create_account(
        &context.payer.pubkey(),
        &mint.pubkey(),
        rent.minimum_balance(Mint::LEN),
        Mint::LEN as u64,
        &spl_token_interface::ID,
    );
    let initialize = instruction::initialize_mint2(
        &spl_token_interface::ID,
        &mint.pubkey(),
        &authority,
        freeze_authority.as_ref(),
        decimals,
    )
    .expect("valid SPL Token program and instruction accounts");
    send(context, &[create, initialize], &[mint]).await
}

/// Install initialized synthetic mint state at an application's configured mint.
pub async fn copy_mint(
    context: &mut ProgramTestContext,
    source: Address,
    destination: Address,
) -> Result<(), BanksClientError> {
    let account = context
        .banks_client
        .get_account(source)
        .await?
        .ok_or(BanksClientError::ClientError("initialized mint missing"))?;
    context.set_account(&destination, &AccountSharedData::from(account));
    Ok(())
}

pub async fn create_account(
    context: &mut ProgramTestContext,
    account: &Keypair,
    owner: Address,
    mint: Address,
) -> Result<(), BanksClientError> {
    let rent = context.banks_client.get_rent().await?;
    let create = solana_system_interface::instruction::create_account(
        &context.payer.pubkey(),
        &account.pubkey(),
        rent.minimum_balance(Account::LEN),
        Account::LEN as u64,
        &spl_token_interface::ID,
    );
    let initialize = instruction::initialize_account3(
        &spl_token_interface::ID,
        &account.pubkey(),
        &mint,
        &owner,
    )
    .expect("valid SPL Token program and instruction accounts");
    send(context, &[create, initialize], &[account]).await
}

pub async fn mint_to(
    context: &mut ProgramTestContext,
    mint: Address,
    destination: Address,
    authority: &Keypair,
    amount: u64,
    decimals: u8,
) -> Result<(), BanksClientError> {
    let instruction = instruction::mint_to_checked(
        &spl_token_interface::ID,
        &mint,
        &destination,
        &authority.pubkey(),
        &[],
        amount,
        decimals,
    )
    .expect("valid SPL Token program and instruction accounts");
    send(context, &[instruction], &[authority]).await
}
