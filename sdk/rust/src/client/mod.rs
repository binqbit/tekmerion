//! Optional asynchronous RPC integration. The caller supplies the RPC endpoint,
//! commitment, wallet signers and cancellation; account validation stays in views.

mod error;
mod reads;

use std::{future::Future, time::Duration};

use solana_message::Message;
use solana_rpc_client::api::{
    config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig},
    response::{Response, RpcSimulateTransactionResult},
};
use solana_sanitize::Sanitize;
use tokio::sync::watch;

pub use error::{ClientError, Result};
pub use solana_rpc_client::{
    api::config::CommitmentConfig, nonblocking::rpc_client::RpcClient, rpc_client::RpcClientConfig,
};
pub use solana_signature::Signature;
pub use solana_signer::Signer;
pub use solana_transaction::Transaction;

use crate::{
    Instruction, Pubkey,
    accounts::{CaseView, PolicyView},
    instructions::CaseAddress,
};

fn validate_transaction(transaction: &Transaction) -> Result<()> {
    transaction
        .sanitize()
        .map_err(|_| ClientError::InvalidTransaction("invalid message structure"))?;
    if transaction.signatures.is_empty()
        || transaction.signatures.len()
            != usize::from(transaction.message.header.num_required_signatures)
    {
        return Err(ClientError::InvalidTransaction(
            "signature count does not match message",
        ));
    }
    let size = wincode::serialized_size(transaction)
        .map_err(|_| ClientError::InvalidTransaction("message cannot be encoded"))?;
    if size > 1232 {
        return Err(ClientError::InvalidTransaction(
            "serialized transaction exceeds 1232 bytes",
        ));
    }
    transaction.verify().map_err(ClientError::InvalidSignature)
}

/// Contract reads and transaction submission through a caller-configured RPC.
pub struct TekmerionClient {
    rpc: RpcClient,
}

impl TekmerionClient {
    pub fn new(rpc: RpcClient) -> Self {
        Self { rpc }
    }

    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// Fetch a recent blockhash and sign this exact message once.
    /// Keep the returned transaction to inspect, simulate, submit or reconcile.
    pub async fn sign_transaction(
        &self,
        instructions: &[Instruction],
        payer: &Pubkey,
        signers: &[&dyn Signer],
    ) -> Result<Transaction> {
        let (blockhash, _) = self
            .rpc
            .get_latest_blockhash_with_commitment(self.rpc.commitment())
            .await?;
        let message = Message::new_with_blockhash(instructions, Some(payer), &blockhash);
        let mut transaction = Transaction::new_unsigned(message);
        transaction.try_sign(signers, blockhash)?;
        validate_transaction(&transaction)?;

        Ok(transaction)
    }

    /// Simulate the signed message without replacing its blockhash or signatures.
    /// Execution errors and logs remain in the returned simulation result.
    pub async fn simulate(
        &self,
        transaction: &Transaction,
    ) -> Result<Response<RpcSimulateTransactionResult>> {
        validate_transaction(transaction)?;
        Ok(self
            .rpc
            .simulate_transaction_with_config(
                transaction,
                RpcSimulateTransactionConfig {
                    sig_verify: true,
                    replace_recent_blockhash: false,
                    commitment: Some(self.rpc.commitment()),
                    ..RpcSimulateTransactionConfig::default()
                },
            )
            .await?)
    }

    /// Submit the supplied signed transaction and poll its confirmation.
    /// This never refreshes a blockhash, re-signs or creates a replacement.
    /// The RPC transport may retry an identical request, preserving signed bytes.
    /// An RPC/confirmation error does not establish that execution failed:
    /// reconcile using the transaction's existing signature before any new action.
    pub async fn send_and_confirm(&self, transaction: &Transaction) -> Result<Signature> {
        validate_transaction(transaction)?;
        Ok(self
            .rpc
            .send_and_confirm_transaction_with_config(
                transaction,
                self.rpc.commitment(),
                RpcSendTransactionConfig {
                    skip_preflight: false,
                    preflight_commitment: Some(self.rpc.commitment().commitment),
                    max_retries: Some(0),
                    ..RpcSendTransactionConfig::default()
                },
            )
            .await?)
    }

    /// Poll validated Case snapshots until cancellation is true or its sender
    /// closes. Cancellation also interrupts an in-flight read. Observations are
    /// snapshots at the RPC commitment, not a complete event history. Older
    /// slots from a lagging RPC node are skipped after a newer slot was delivered.
    pub async fn watch_case(
        &self,
        case: CaseAddress,
        interval: Duration,
        cancel: watch::Receiver<bool>,
        on_update: impl FnMut(CaseView),
    ) -> Result<()> {
        watch_snapshots(
            interval,
            cancel,
            || self.fetch_case(case),
            |view| view.observation().slot,
            on_update,
        )
        .await
    }

    /// Poll an independently validated Policy, including before any Case exists.
    /// Cancellation and observation semantics are the same as `watch_case`.
    pub async fn watch_policy(
        &self,
        policy: Pubkey,
        interval: Duration,
        cancel: watch::Receiver<bool>,
        on_update: impl FnMut(PolicyView),
    ) -> Result<()> {
        watch_snapshots(
            interval,
            cancel,
            || self.fetch_policy(policy),
            |view| view.observation().slot,
            on_update,
        )
        .await
    }
}

async fn watch_snapshots<T, F: Future<Output = Result<T>>>(
    interval: Duration,
    mut cancel: watch::Receiver<bool>,
    mut fetch: impl FnMut() -> F,
    observed_slot: impl Fn(&T) -> u64,
    mut on_update: impl FnMut(T),
) -> Result<()> {
    if interval.is_zero() {
        return Err(ClientError::InvalidPollInterval);
    }
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_delivered_slot = None;

    loop {
        if *cancel.borrow() {
            return Ok(());
        }
        tokio::select! {
            biased;
            changed = cancel.changed() => {
                if changed.is_err() || *cancel.borrow() { return Ok(()); }
            }
            _ = ticker.tick() => {
                tokio::select! {
                    biased;
                    changed = cancel.changed() => {
                        if changed.is_err() || *cancel.borrow() { return Ok(()); }
                    }
                    view = fetch() => {
                        let view = view?;
                        let slot = observed_slot(&view);
                        if last_delivered_slot.is_some_and(|previous| slot < previous) {
                            continue;
                        }
                        last_delivered_slot = Some(slot);
                        on_update(view);
                    }
                }
            }
        }
    }
}
