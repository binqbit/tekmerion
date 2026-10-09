#![cfg(feature = "rpc")]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use solana_keypair::Keypair;
use solana_rpc_client::{
    api::{client_error, request::RpcRequest},
    rpc_sender::{RpcSender, RpcTransportStats},
};
use tekmerion_sdk::{
    Instruction, Pubkey, SdkError,
    accounts::ClaimState,
    client::{
        ClientError, CommitmentConfig, RpcClient, RpcClientConfig, Signer, TekmerionClient,
        Transaction,
    },
    instructions::{CaseAddress, finalize_timeout},
};

type Calls = Arc<Mutex<Vec<(RpcRequest, Value)>>>;

struct ScriptedSender {
    calls: Calls,
    replies: Mutex<VecDeque<(RpcRequest, Value)>>,
}

#[async_trait]
impl RpcSender for ScriptedSender {
    async fn send(&self, request: RpcRequest, params: Value) -> client_error::Result<Value> {
        self.calls.lock().unwrap().push((request, params.clone()));
        let (expected, reply) = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected RPC call");
        assert_eq!(request, expected);
        if request == RpcRequest::SendTransaction && reply.is_null() {
            let bytes = STANDARD.decode(params[0].as_str().unwrap()).unwrap();
            let transaction: Transaction = wincode::deserialize(&bytes).unwrap();
            transaction.verify().unwrap();
            return Ok(json!(transaction.signatures[0].to_string()));
        }
        Ok(reply)
    }

    fn get_transport_stats(&self) -> RpcTransportStats {
        RpcTransportStats::default()
    }
    fn url(&self) -> String {
        "mock://tekmerion".into()
    }
}

fn client(replies: Vec<(RpcRequest, Value)>) -> (TekmerionClient, Calls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let rpc = RpcClient::new_sender(
        ScriptedSender {
            calls: calls.clone(),
            replies: Mutex::new(replies.into()),
        },
        RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
    );
    (TekmerionClient::new(rpc), calls)
}

fn blockhash() -> Value {
    json!({"context":{"slot":100},"value":{
        "blockhash":"11111111111111111111111111111111", "lastValidBlockHeight":200
    }})
}

fn case_address() -> CaseAddress {
    CaseAddress {
        policy: Pubkey::new_from_array([41; 32]),
        subject: Pubkey::new_from_array([42; 32]),
        nonce: 0,
    }
}

#[tokio::test]
async fn signing_simulation_and_submission_preserve_the_exact_reviewed_transaction() {
    let (client, calls) = client(vec![
        (RpcRequest::GetLatestBlockhash, blockhash()),
        (
            RpcRequest::SimulateTransaction,
            json!({"context":{"slot":100},"value":{
                "err":null,"logs":["mock simulation"],"accounts":null,
                "unitsConsumed":1234,"returnData":null,"innerInstructions":null,
                "replacementBlockhash":null
            }}),
        ),
        (RpcRequest::SendTransaction, Value::Null),
        (
            RpcRequest::GetSignatureStatuses,
            json!({"context":{"slot":101},"value":[{
                "slot":100,"confirmations":1,"err":null,"status":{"Ok":null},
                "confirmationStatus":"confirmed"
            }]}),
        ),
    ]);
    let payer = Keypair::new_from_array([71; 32]);
    let instruction = finalize_timeout(case_address());

    let transaction = client
        .sign_transaction(&[instruction], &payer.pubkey(), &[&payer])
        .await
        .unwrap();
    let original = wincode::serialize(&transaction).unwrap();
    let simulation = client.simulate(&transaction).await.unwrap();
    let signature = client.send_and_confirm(&transaction).await.unwrap();

    assert_eq!(simulation.value.units_consumed, Some(1234));
    assert_eq!(signature, transaction.signatures[0]);
    assert_eq!(wincode::serialize(&transaction).unwrap(), original);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        STANDARD.decode(calls[1].1[0].as_str().unwrap()).unwrap(),
        original
    );
    assert_eq!(
        STANDARD.decode(calls[2].1[0].as_str().unwrap()).unwrap(),
        original
    );
    assert_eq!(calls[1].1[1]["sigVerify"], true);
    assert_eq!(calls[1].1[1]["replaceRecentBlockhash"], false);
    assert_eq!(calls[2].1[1]["maxRetries"], 0);
    assert_eq!(calls[2].1[1]["skipPreflight"], false);
    assert_eq!(calls[2].1[1]["preflightCommitment"], "confirmed");
    assert_eq!(calls[3].1[0][0], signature.to_string());
}

#[tokio::test]
async fn execution_failure_during_confirmation_is_returned_without_resending() {
    let execution_error = json!({"InstructionError":[0,{"Custom":6000}]});
    let (client, calls) = client(vec![
        (RpcRequest::GetLatestBlockhash, blockhash()),
        (RpcRequest::SendTransaction, Value::Null),
        (
            RpcRequest::GetSignatureStatuses,
            json!({"context":{"slot":101},"value":[{
                "slot":100,"confirmations":1,"err":execution_error,
                "status":{"Err":execution_error},"confirmationStatus":"confirmed"
            }]}),
        ),
    ]);
    let payer = Keypair::new_from_array([75; 32]);
    let transaction = client
        .sign_transaction(
            &[finalize_timeout(case_address())],
            &payer.pubkey(),
            &[&payer],
        )
        .await
        .unwrap();
    let original = wincode::serialize(&transaction).unwrap();

    let result = client.send_and_confirm(&transaction).await;

    match result {
        Err(ClientError::Rpc(error)) => assert!(error.kind().get_transaction_error().is_some()),
        other => panic!("expected execution error, got {other:?}"),
    }
    assert_eq!(wincode::serialize(&transaction).unwrap(), original);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls
            .iter()
            .filter(|(request, _)| *request == RpcRequest::SendTransaction)
            .count(),
        1
    );
    assert_eq!(
        STANDARD.decode(calls[1].1[0].as_str().unwrap()).unwrap(),
        original
    );
}

#[tokio::test]
async fn simulation_returns_execution_error_and_logs_without_submitting() {
    let (client, calls) = client(vec![
        (RpcRequest::GetLatestBlockhash, blockhash()),
        (
            RpcRequest::SimulateTransaction,
            json!({"context":{"slot":100},"value":{
                "err":{"InstructionError":[0,{"Custom":6000}]},
                "logs":["mock execution rejected"],"accounts":null,"unitsConsumed":123,
                "returnData":null,"innerInstructions":null,"replacementBlockhash":null
            }}),
        ),
    ]);
    let payer = Keypair::new_from_array([76; 32]);
    let transaction = client
        .sign_transaction(
            &[finalize_timeout(case_address())],
            &payer.pubkey(),
            &[&payer],
        )
        .await
        .unwrap();

    let simulation = client.simulate(&transaction).await.unwrap();

    assert!(simulation.value.err.is_some());
    assert_eq!(
        simulation.value.logs,
        Some(vec!["mock execution rejected".into()])
    );
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn modified_signed_messages_never_reach_simulation_or_submission() {
    let (client, calls) = client(vec![(RpcRequest::GetLatestBlockhash, blockhash())]);
    let payer = Keypair::new_from_array([72; 32]);
    let mut transaction = client
        .sign_transaction(
            &[finalize_timeout(case_address())],
            &payer.pubkey(),
            &[&payer],
        )
        .await
        .unwrap();
    transaction.message.instructions[0].data[0] ^= 1;

    let simulation = client.simulate(&transaction).await;
    let submission = client.send_and_confirm(&transaction).await;

    assert!(matches!(simulation, Err(ClientError::InvalidSignature(_))));
    assert!(matches!(submission, Err(ClientError::InvalidSignature(_))));
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_and_incomplete_transactions_are_rejected_before_rpc_submission() {
    let payer = Keypair::new_from_array([74; 32]);
    let oversized = Instruction {
        program_id: tekmerion_sdk::ID,
        accounts: vec![],
        data: vec![0; 1232],
    };
    let (client, calls) = client(vec![(RpcRequest::GetLatestBlockhash, blockhash())]);

    let signed = client
        .sign_transaction(std::slice::from_ref(&oversized), &payer.pubkey(), &[&payer])
        .await;

    assert!(matches!(signed, Err(ClientError::InvalidTransaction(_))));
    for variant in 0..3 {
        let instruction = if variant == 0 {
            oversized.clone()
        } else {
            finalize_timeout(case_address())
        };
        let mut transaction = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer.pubkey()),
            &[&payer],
            Default::default(),
        );
        match variant {
            1 => transaction.signatures.clear(),
            2 => transaction.message.instructions[0].program_id_index = u8::MAX,
            _ => {}
        }

        let simulation = client.simulate(&transaction).await;
        let submission = client.send_and_confirm(&transaction).await;

        assert!(matches!(
            simulation,
            Err(ClientError::InvalidTransaction(_))
        ));
        assert!(matches!(
            submission,
            Err(ClientError::InvalidTransaction(_))
        ));
    }
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn cancellation_stops_monitoring_before_any_read() {
    let (client, calls) = client(vec![]);
    let (_sender, cancel) = tokio::sync::watch::channel(true);

    client
        .watch_case(case_address(), Duration::from_secs(60), cancel, |_| {
            panic!("cancelled watcher emitted a snapshot");
        })
        .await
        .unwrap();

    assert!(calls.lock().unwrap().is_empty());
}

struct PendingSender {
    started: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl RpcSender for PendingSender {
    async fn send(&self, _: RpcRequest, _: Value) -> client_error::Result<Value> {
        self.started.notify_one();
        std::future::pending().await
    }
    fn get_transport_stats(&self) -> RpcTransportStats {
        RpcTransportStats::default()
    }
    fn url(&self) -> String {
        "mock://pending-read".into()
    }
}

#[tokio::test]
async fn cancellation_interrupts_an_inflight_account_read() {
    let started = Arc::new(tokio::sync::Notify::new());
    let client = TekmerionClient::new(RpcClient::new_sender(
        PendingSender {
            started: started.clone(),
        },
        RpcClientConfig::default(),
    ));
    let (sender, cancel) = tokio::sync::watch::channel(false);

    let watcher = client.watch_case(case_address(), Duration::from_secs(60), cancel, |_| {
        panic!("pending RPC must not emit a snapshot");
    });
    let cancellation = async {
        started.notified().await;
        sender.send(true).unwrap();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(watcher, cancellation)
    })
    .await
    .expect("cancellation must not wait for RPC response or next poll");

    result.unwrap();
}

fn fixture() -> Value {
    let network = if cfg!(feature = "devnet") {
        "devnet"
    } else {
        "mainnet"
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../fixtures/sdk-{network}.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn snapshot<'a>(fixture: &'a Value, name: &str) -> &'a Value {
    fixture["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|snapshot| snapshot["name"] == name)
        .unwrap()
}

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn ui_account(account: &Value) -> Value {
    json!({
        "data":[STANDARD.encode(decode_hex(account["dataHex"].as_str().unwrap())),"base64"],
        "owner":account["owner"], "executable":account["executable"],
        "lamports":1_000_000, "rentEpoch":0,
    })
}

fn response(snapshot: &Value, names: &[&str]) -> Value {
    let slot: u64 = snapshot["observation"]["slot"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    json!({"context":{"slot":slot},"value":names.iter()
        .map(|name| ui_account(&snapshot["accounts"][name])).collect::<Vec<_>>()})
}

fn identity(snapshot: &Value) -> CaseAddress {
    CaseAddress {
        policy: snapshot["caseAddress"]["policy"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        subject: snapshot["caseAddress"]["subject"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
        nonce: snapshot["caseAddress"]["nonce"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap(),
    }
}

struct AccountSender {
    calls: Calls,
    slot: u64,
    accounts: Vec<Value>,
}

#[async_trait]
impl RpcSender for AccountSender {
    async fn send(&self, request: RpcRequest, params: Value) -> client_error::Result<Value> {
        assert_eq!(request, RpcRequest::GetMultipleAccounts);
        self.calls.lock().unwrap().push((request, params.clone()));
        let accounts: Vec<_> = params[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| {
                self.accounts
                    .iter()
                    .find(|account| account["key"] == *key)
                    .map(ui_account)
                    .unwrap_or(Value::Null)
            })
            .collect();
        Ok(json!({"context":{"slot":self.slot},"value":accounts}))
    }

    fn get_transport_stats(&self) -> RpcTransportStats {
        RpcTransportStats::default()
    }

    fn url(&self) -> String {
        "mock://tekmerion-accounts".into()
    }
}

fn account_client(snapshot: &Value, subject_present: bool) -> (TekmerionClient, Calls) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let accounts = snapshot["accounts"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(name, _)| subject_present || name.as_str() != "subject")
        .map(|(_, account)| account.clone())
        .collect();
    let rpc = RpcClient::new_sender(
        AccountSender {
            calls: calls.clone(),
            slot: snapshot["observation"]["slot"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
            accounts,
        },
        RpcClientConfig::with_commitment(CommitmentConfig::confirmed()),
    );
    (TekmerionClient::new(rpc), calls)
}

#[tokio::test]
async fn case_and_claim_reads_do_not_depend_on_subject_presence() {
    let fixture = fixture();
    let snapshot = snapshot(&fixture, "no-finalized");
    let case = identity(snapshot);
    for subject_present in [true, false] {
        let (client, calls) = account_client(snapshot, subject_present);
        let view = client.fetch_case(case).await.unwrap();
        assert_eq!(view.case().subject, case.subject);
        assert_eq!(
            view.vault_balance().to_string(),
            snapshot["expected"]["vaultBalance"]
        );
        for owner in ["author", "alice"] {
            let key = fixture["actors"][owner].as_str().unwrap().parse().unwrap();
            let (view, claim) = client.fetch_claim(case, key).await.unwrap();
            let expected = snapshot["expected"][format!("{owner}Claim")]["amount"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(claim, ClaimState::Unclaimed { amount: expected });
            assert_eq!(view.case().subject, case.subject);
        }
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 6);
        assert!(calls.iter().all(|(_, params)| {
            !params[0]
                .as_array()
                .unwrap()
                .contains(&json!(case.subject.to_string()))
        }));
    }
}

#[tokio::test]
async fn case_monitor_does_not_depend_on_subject_presence() {
    let fixture = fixture();
    let snapshot = snapshot(&fixture, "no-finalized");
    let case = identity(snapshot);
    for subject_present in [true, false] {
        let (client, calls) = account_client(snapshot, subject_present);
        let (sender, cancel) = tokio::sync::watch::channel(false);
        let mut emitted = 0;
        client
            .watch_case(case, Duration::from_secs(60), cancel, |view| {
                assert_eq!(view.case().subject, case.subject);
                assert_eq!(
                    view.vault_balance().to_string(),
                    snapshot["expected"]["vaultBalance"]
                );
                emitted += 1;
                sender.send(true).unwrap();
            })
            .await
            .unwrap();
        assert_eq!(emitted, 1);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().all(|(_, params)| {
            !params[0]
                .as_array()
                .unwrap()
                .contains(&json!(case.subject.to_string()))
        }));
    }
}

#[tokio::test]
async fn case_and_claim_reads_still_require_every_protocol_account_and_clock() {
    let fixture = fixture();
    let snapshot = snapshot(&fixture, "no-finalized");
    let case = identity(snapshot);
    for owner in [None, Some("author"), Some("alice")] {
        let mut required = CASE_ACCOUNTS.to_vec();
        if owner == Some("alice") {
            required.push("aliceReceipt");
        }
        for missing in required {
            let missing_key: Pubkey = snapshot["accounts"][missing]["key"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap();
            let mut incomplete = snapshot.clone();
            incomplete["accounts"]
                .as_object_mut()
                .unwrap()
                .remove(missing);
            let (client, _) = account_client(&incomplete, false);
            let result = if let Some(owner) = owner {
                let key = fixture["actors"][owner].as_str().unwrap().parse().unwrap();
                client.fetch_claim(case, key).await.map(|(view, _)| view)
            } else {
                client.fetch_case(case).await
            };
            assert!(
                matches!(result, Err(ClientError::MissingAccount(key)) if key == missing_key),
                "missing {missing}, claimant {owner:?}: {result:?}"
            );
        }
    }
}

const CASE_ACCOUNTS: [&str; 5] = ["case", "policy", "vault", "authorReceipt", "clock"];

#[tokio::test]
async fn policy_read_batches_clock_and_accepts_a_policy_without_cases() {
    let fixture = fixture();
    let before = &fixture["policyBeforeCase"];
    let slot: u64 = before["observation"]["slot"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let key: Pubkey = before["account"]["key"].as_str().unwrap().parse().unwrap();
    let (client, calls) = client(vec![(
        RpcRequest::GetMultipleAccounts,
        json!({
            "context":{"slot":slot}, "value":[ui_account(&before["account"]),ui_account(&before["clockAccount"])]
        }),
    )]);

    let view = client.fetch_policy(key).await.unwrap();

    assert_eq!(view.address(), key);
    assert_eq!(view.policy().active_uses, 0);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].1[0],
        json!([key.to_string(), before["clockAccount"]["key"]])
    );
    assert_eq!(calls[0].1[1]["commitment"], "confirmed");
}

#[tokio::test]
async fn case_and_both_claim_roles_use_one_complete_final_snapshot() {
    let fixture = fixture();
    for name in [
        "funded",
        "no-finalized",
        "no-alice-claimed",
        "no-all-claimed",
    ] {
        let snapshot = snapshot(&fixture, name);
        let case = identity(snapshot);
        for owner_name in [None, Some("author"), Some("alice")] {
            let mut names = CASE_ACCOUNTS.to_vec();
            if let Some(owner) = owner_name {
                names.push(if owner == "author" {
                    "authorReceipt"
                } else {
                    "aliceReceipt"
                });
            }
            let (client, calls) = client(vec![
                (
                    RpcRequest::GetMultipleAccounts,
                    response(snapshot, &["case"]),
                ),
                (RpcRequest::GetMultipleAccounts, response(snapshot, &names)),
            ]);

            let view = if let Some(owner) = owner_name {
                let owner_key = fixture["actors"][owner].as_str().unwrap().parse().unwrap();
                let (view, quote) = client.fetch_claim(case, owner_key).await.unwrap();
                let actual = match quote {
                    ClaimState::Unavailable => json!({"kind":"unavailable"}),
                    ClaimState::Claimed => json!({"kind":"claimed"}),
                    ClaimState::Unclaimed { amount } => {
                        json!({"kind":"unclaimed","amount":amount.to_string()})
                    }
                };
                assert_eq!(actual, snapshot["expected"][format!("{owner}Claim")]);
                view
            } else {
                client.fetch_case(case).await.unwrap()
            };

            assert_eq!(
                view.vault_balance().to_string(),
                snapshot["expected"]["vaultBalance"]
            );
            let calls = calls.lock().unwrap();
            assert_eq!(calls.len(), 2);
            assert_eq!(calls[0].1[0], json!([case.address().to_string()]));
            let expected_keys: Vec<_> = names
                .iter()
                .map(|name| snapshot["accounts"][name]["key"].clone())
                .collect();
            assert_eq!(calls[1].1[0], json!(expected_keys));
            assert_eq!(calls[1].1[1]["minContextSlot"], view.observation().slot);
            assert_eq!(calls[1].1[1]["commitment"], "confirmed");
        }
    }
}

#[tokio::test]
async fn reads_reject_context_rollback_missing_accounts_and_clock_mismatch() {
    let fixture = fixture();
    let snapshot = snapshot(&fixture, "funded");
    for variant in 0..5 {
        let mut final_response = response(snapshot, &CASE_ACCOUNTS);
        match variant {
            0 => {
                final_response["context"]["slot"] =
                    json!(final_response["context"]["slot"].as_u64().unwrap() - 1)
            }
            1 => final_response["value"][3] = Value::Null,
            2 => {
                final_response["context"]["slot"] =
                    json!(final_response["context"]["slot"].as_u64().unwrap() + 1)
            }
            3 => final_response["value"][4]["owner"] = json!(tekmerion_sdk::ID.to_string()),
            _ => {
                final_response["value"].as_array_mut().unwrap().pop();
            }
        }
        let (client, _) = client(vec![
            (
                RpcRequest::GetMultipleAccounts,
                response(snapshot, &["case"]),
            ),
            (RpcRequest::GetMultipleAccounts, final_response),
        ]);

        let result = client.fetch_case(identity(snapshot)).await;

        match variant {
            0 | 2 => assert!(matches!(
                result,
                Err(ClientError::Validation(SdkError::InconsistentObservation))
            )),
            1 => assert!(matches!(result, Err(ClientError::MissingAccount(_)))),
            3 => assert!(matches!(
                result,
                Err(ClientError::Validation(SdkError::InvalidAccount))
            )),
            _ => assert!(matches!(result, Err(ClientError::MalformedResponse(_)))),
        }
    }
}

#[tokio::test]
async fn monitoring_delivers_a_validated_snapshot_then_stops_without_an_extra_read() {
    let fixture = fixture();
    let snapshot = snapshot(&fixture, "funded");
    let (client, calls) = client(vec![
        (
            RpcRequest::GetMultipleAccounts,
            response(snapshot, &["case"]),
        ),
        (
            RpcRequest::GetMultipleAccounts,
            response(snapshot, &CASE_ACCOUNTS),
        ),
    ]);
    let (sender, cancel) = tokio::sync::watch::channel(false);
    let mut emitted = 0;

    client
        .watch_case(
            identity(snapshot),
            Duration::from_secs(60),
            cancel,
            |view| {
                assert_eq!(
                    view.vault_balance().to_string(),
                    snapshot["expected"]["vaultBalance"]
                );
                emitted += 1;
                sender.send(true).unwrap();
            },
        )
        .await
        .unwrap();

    assert_eq!(emitted, 1);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn policy_monitor_skips_older_valid_snapshots_from_a_lagging_node() {
    let fixture = fixture();
    let before = &fixture["policyBeforeCase"];
    let key: Pubkey = before["account"]["key"].as_str().unwrap().parse().unwrap();
    let bytes = decode_hex(before["clockAccount"]["dataHex"].as_str().unwrap());
    let mut clock: anchor_lang::prelude::Clock = bincode::deserialize(&bytes).unwrap();
    let first_slot = clock.slot;
    let replies = [first_slot, first_slot - 1, first_slot + 1]
        .into_iter()
        .map(|slot| {
            clock.slot = slot;
            let mut clock_account = ui_account(&before["clockAccount"]);
            clock_account["data"][0] = json!(STANDARD.encode(bincode::serialize(&clock).unwrap()));
            (
                RpcRequest::GetMultipleAccounts,
                json!({"context":{"slot":slot},"value":[
                    ui_account(&before["account"]),clock_account
                ]}),
            )
        })
        .collect();
    let (client, calls) = client(replies);
    let (sender, cancel) = tokio::sync::watch::channel(false);
    let mut delivered = Vec::new();

    let watcher = client.watch_policy(key, Duration::from_millis(1), cancel, |view| {
        delivered.push(view.observation().slot);
        if view.observation().slot == first_slot + 1 {
            sender.send(true).unwrap();
        }
    });
    tokio::time::timeout(Duration::from_secs(1), watcher)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(delivered, [first_slot, first_slot + 1]);
    assert_eq!(calls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn changed_author_between_discovery_and_snapshot_is_rejected() {
    use anchor_lang::{AccountDeserialize, AccountSerialize};

    let fixture = fixture();
    let snapshot = snapshot(&fixture, "funded");
    let mut preliminary = response(snapshot, &["case"]);
    let data = decode_hex(snapshot["accounts"]["case"]["dataHex"].as_str().unwrap());
    let mut case = tekmerion_sdk::program::Case::try_deserialize(&mut data.as_slice()).unwrap();
    case.author = Pubkey::new_from_array([99; 32]);
    let mut altered = Vec::new();
    case.try_serialize(&mut altered).unwrap();
    altered.resize(tekmerion_sdk::program::Case::SPACE, 0);
    preliminary["value"][0]["data"][0] = json!(STANDARD.encode(altered));
    let (client, _) = client(vec![
        (RpcRequest::GetMultipleAccounts, preliminary),
        (
            RpcRequest::GetMultipleAccounts,
            response(snapshot, &CASE_ACCOUNTS),
        ),
    ]);

    let result = client.fetch_case(identity(snapshot)).await;

    assert!(matches!(
        result,
        Err(ClientError::Validation(SdkError::InvalidRelationship))
    ));
}
