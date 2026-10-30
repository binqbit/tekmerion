//! VM-independent synthetic oracle transport for the pinned upstream Forwarder.
//! Test-only keys; never use this deployment as a production oracle.
use anchor_lang::{AnchorSerialize, prelude::Pubkey};
use k256::ecdsa::SigningKey;
use sha2::{Digest, Sha256};
use sha3::Keccak256;
use solana_instruction::{AccountMeta, Instruction};
use tekmerion_chainlink_sdk::{self as sdk, confidential::ConfidentialConfig};
pub const FORWARDER: Pubkey = Pubkey::from_str_const("whV7Q5pi17hPPyaPksToDw1nMx6Lh8qmNWKFaLRQ4wz");
pub const DON: u32 = 1;
pub const CONFIG_VERSION: u32 = 1;
pub fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn discriminator(method: &str) -> Vec<u8> {
    sha(format!("global:{method}").as_bytes())[..8].to_vec()
}
pub fn serialize(value: &impl AnchorSerialize) -> Vec<u8> {
    let mut b = vec![];
    value.serialize(&mut b).unwrap();
    b
}
pub fn oracle_keys() -> Vec<SigningKey> {
    (1u8..=4)
        .map(|n| SigningKey::from_bytes((&[n; 32]).into()).unwrap())
        .collect()
}
pub fn oracle_address(key: &SigningKey) -> [u8; 20] {
    Keccak256::digest(&key.verifying_key().to_encoded_point(false).as_bytes()[1..])[12..]
        .try_into()
        .unwrap()
}
pub fn signed_report_with_accounts(
    config: &ConfidentialConfig,
    receiver: Vec<AccountMeta>,
    payer: Pubkey,
    oracles: Pubkey,
    raw: Vec<u8>,
    signers: &[usize],
) -> Instruction {
    let context = [0u8; 96];
    let digest = sha(&[&[raw.len() as u8], raw.as_slice(), context.as_slice()].concat());
    let mut envelope = vec![signers.len() as u8];
    let keys = oracle_keys();
    for i in signers {
        let (sig, rec) = keys[*i].sign_prehash_recoverable(&digest).unwrap();
        envelope.extend(sig.to_bytes());
        envelope.push(rec.to_byte());
    }
    envelope.extend(&raw);
    envelope.extend(context);
    let transmission = sha(&[sdk::ID.as_ref(), &raw[1..33], &raw[107..109]].concat());
    let execution = Pubkey::find_program_address(
        &[
            b"execution_state",
            config.forwarder_state.as_ref(),
            &transmission,
        ],
        &FORWARDER,
    )
    .0;
    let mut accounts = vec![
        receiver[0].clone(),
        AccountMeta::new_readonly(oracles, false),
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(receiver[1].pubkey, false),
        AccountMeta::new(execution, false),
        AccountMeta::new_readonly(sdk::ID, false),
        AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
    ];
    accounts.extend_from_slice(&receiver[2..]);
    let mut data = discriminator("report");
    data.extend(serialize(&envelope));
    Instruction {
        program_id: FORWARDER,
        accounts,
        data,
    }
}
