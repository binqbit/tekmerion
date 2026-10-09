use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_program_test::{BanksClientError, ProgramTest, ProgramTestContext};
use solana_signer::Signer;
use solana_transaction::Transaction;

/// Start a directly usable Solana bank after registering programs and accounts.
pub async fn start(program: ProgramTest, clock: Clock) -> ProgramTestContext {
    let context = program.start_with_context().await;
    context.set_sysvar(&clock);
    context
}

/// Install independent wallet or application state before the bank starts.
pub fn add_system_account(program: &mut ProgramTest, address: Address, lamports: u64) {
    program.add_account(
        address,
        Account {
            lamports,
            owner: solana_system_interface::program::ID,
            ..Account::default()
        },
    );
}

/// Submit setup instructions with the bank's payer and explicit additional keys.
pub async fn send(
    context: &mut ProgramTestContext,
    instructions: &[Instruction],
    additional: &[&Keypair],
) -> Result<(), BanksClientError> {
    context.last_blockhash = context.banks_client.get_latest_blockhash().await?;
    let mut transaction = signed_transaction(context, instructions, additional);
    if context
        .banks_client
        .get_transaction_status(transaction.signatures[0])
        .await?
        .is_some()
    {
        context.get_new_latest_blockhash().await?;
        transaction = signed_transaction(context, instructions, additional);
    }
    context.banks_client.process_transaction(transaction).await
}

fn signed_transaction(
    context: &ProgramTestContext,
    instructions: &[Instruction],
    additional: &[&Keypair],
) -> Transaction {
    let mut signers = vec![&context.payer];
    signers.extend_from_slice(additional);
    Transaction::new_signed_with_payer(
        instructions,
        Some(&context.payer.pubkey()),
        &signers,
        context.last_blockhash,
    )
}

pub async fn fund(
    context: &mut ProgramTestContext,
    address: Address,
    lamports: u64,
) -> Result<(), BanksClientError> {
    let instruction =
        solana_system_interface::instruction::transfer(&context.payer.pubkey(), &address, lamports);
    send(context, &[instruction], &[]).await
}
