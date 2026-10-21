use anchor_lang::{prelude::Clock, solana_program::sysvar::SysvarId};
use solana_account::Account;
use solana_rpc_client::api::config::{RpcAccountInfoConfig, UiAccountEncoding};

use super::{ClientError, Result, TekmerionClient};
use crate::{
    ID, Pubkey, SdkError,
    accounts::{AccountData, CaseView, ClaimState, Observation, PolicyView},
    instructions::CaseAddress,
    program::Case,
};

struct Snapshot {
    slot: u64,
    accounts: Vec<Account>,
}

impl Snapshot {
    fn account(&self, index: usize, key: Pubkey) -> AccountData<'_> {
        let account = &self.accounts[index];
        AccountData {
            key,
            owner: account.owner,
            executable: account.executable,
            data: &account.data,
            slot: self.slot,
        }
    }

    fn observation(&self, clock_index: usize) -> Result<Observation> {
        let account = &self.accounts[clock_index];
        if account.owner != solana_sdk_ids::sysvar::ID
            || account.executable
            || account.data.len() != 40
        {
            return Err(SdkError::InvalidAccount.into());
        }
        let clock: Clock = bincode::deserialize(&account.data)
            .map_err(|_| ClientError::Validation(SdkError::InvalidAccount))?;
        if clock.slot != self.slot {
            return Err(SdkError::InconsistentObservation.into());
        }
        Ok(Observation {
            slot: self.slot,
            unix_timestamp: clock.unix_timestamp,
        })
    }
}

impl TekmerionClient {
    /// Fetch Policy and chain Clock from the same RPC bank context.
    pub async fn fetch_policy(&self, expected: Pubkey) -> Result<PolicyView> {
        let snapshot = self.snapshot(&[expected, Clock::id()], None).await?;
        Ok(PolicyView::read(
            snapshot.observation(1)?,
            expected,
            snapshot.account(0, expected),
        )?)
    }

    /// Refetch Case, Policy, vault, author receipt and Clock together
    /// after discovering the immutable author needed to derive the receipt.
    /// The opaque subject identity is authenticated from Case, not fetched.
    pub async fn fetch_case(&self, expected: CaseAddress) -> Result<CaseView> {
        Ok(self.case_snapshot(expected, None).await?.0)
    }

    /// Read a current payout quote with both claim markers and vault backing
    /// from one final batch. The preliminary author lookup is not a quote.
    pub async fn fetch_claim(
        &self,
        expected: CaseAddress,
        owner: Pubkey,
    ) -> Result<(CaseView, ClaimState)> {
        let (view, claim) = self.case_snapshot(expected, Some(owner)).await?;
        Ok((view, claim.expect("requested claim is always computed")))
    }

    async fn case_snapshot(
        &self,
        expected: CaseAddress,
        claimant: Option<Pubkey>,
    ) -> Result<(CaseView, Option<ClaimState>)> {
        let case_key = expected.address();
        let first = self.snapshot(&[case_key], None).await?;
        let preliminary: Case = first.account(0, case_key).decode(
            Observation {
                slot: first.slot,
                unix_timestamp: 0,
            },
            ID,
            Case::SPACE,
        )?;
        if preliminary.policy != expected.policy
            || preliminary.subject != expected.subject
            || preliminary.author != expected.author
            || preliminary.nonce != expected.nonce
            || Case::address(
                &expected.policy,
                &expected.subject,
                &expected.author,
                expected.nonce,
            )
            .1 != preliminary.bump
        {
            return Err(SdkError::InvalidRelationship.into());
        }
        let author = preliminary.author;
        let mut keys = vec![
            case_key,
            expected.policy,
            expected.vault(),
            expected.receipt(&author),
            Clock::id(),
        ];
        if let Some(owner) = claimant {
            keys.push(expected.receipt(&owner));
        }
        let snapshot = self.snapshot(&keys, Some(first.slot)).await?;
        let view = CaseView::read(
            snapshot.observation(4)?,
            expected,
            snapshot.account(0, keys[0]),
            snapshot.account(1, keys[1]),
            None,
            snapshot.account(2, keys[2]),
            snapshot.account(3, keys[3]),
        )?;
        if view.case().author != author {
            return Err(SdkError::InvalidRelationship.into());
        }
        let claim = claimant
            .map(|owner| {
                if owner == author {
                    Ok(view.author_claim())
                } else {
                    view.contributor_claim(owner, snapshot.account(5, keys[5]))
                }
            })
            .transpose()?;

        Ok((view, claim))
    }

    async fn snapshot(&self, keys: &[Pubkey], min_context_slot: Option<u64>) -> Result<Snapshot> {
        let response = self
            .rpc
            .get_multiple_ui_accounts_with_config(
                keys,
                RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(self.rpc.commitment()),
                    data_slice: None,
                    min_context_slot,
                },
            )
            .await?;
        if response.value.len() != keys.len() {
            return Err(ClientError::MalformedResponse(
                "account count does not match request",
            ));
        }
        if min_context_slot.is_some_and(|minimum| response.context.slot < minimum) {
            return Err(SdkError::InconsistentObservation.into());
        }
        let accounts = response
            .value
            .into_iter()
            .zip(keys)
            .map(|(account, key)| {
                account
                    .ok_or(ClientError::MissingAccount(*key))?
                    .to_account()
                    .ok_or(ClientError::Validation(SdkError::InvalidAccount))
            })
            .collect::<Result<Vec<Account>>>()?;

        Ok(Snapshot {
            slot: response.context.slot,
            accounts,
        })
    }
}
