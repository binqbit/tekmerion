use solana_clock::Clock;
use solana_keypair::Keypair;
use solana_program_pack::Pack;
use solana_program_test::ProgramTest;
use solana_signer::Signer;
use spl_token_interface::state::{Account, Mint};
use tokio::runtime::Builder;

use crate::support::setup::{context, token};

#[test]
fn reusable_setup_applies_identical_mints_twice_without_changing_clock() {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let authority = Keypair::new_from_array([0x71; 32]);
            let mint = Keypair::new_from_array([0x72; 32]);
            let destination = Keypair::new_from_array([0x73; 32]);
            let mut program = ProgramTest::default();
            context::add_system_account(&mut program, authority.pubkey(), 2_000_000_000);
            let clock = Clock {
                slot: 100,
                unix_timestamp: 1_000,
                ..Clock::default()
            };
            let mut bank = context::start(program, clock.clone()).await;
            token::create_mint(&mut bank, &mint, authority.pubkey(), None, 6)
                .await
                .unwrap();
            token::create_account(&mut bank, &destination, authority.pubkey(), mint.pubkey())
                .await
                .unwrap();
            for _ in 0..2 {
                token::mint_to(
                    &mut bank,
                    mint.pubkey(),
                    destination.pubkey(),
                    &authority,
                    7,
                    6,
                )
                .await
                .unwrap();
            }
            let account = bank
                .banks_client
                .get_account(destination.pubkey())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(Account::unpack(&account.data).unwrap().amount, 14);
            let account = bank
                .banks_client
                .get_account(mint.pubkey())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(Mint::unpack(&account.data).unwrap().supply, 14);
            assert_eq!(
                bank.banks_client.get_sysvar::<Clock>().await.unwrap(),
                clock
            );
        });
}
