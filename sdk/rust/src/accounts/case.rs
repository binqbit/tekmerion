use anchor_spl::token::{self, TokenAccount};
use tekmerion::calculations::{
    amounts::UsdcAmount,
    funding::FundingSnapshot,
    time::{ChainDeadline, ChainTimestamp},
};
use tekmerion::{AUTHOR_SIDE, Case, Policy, StakeReceipt};

use super::{AccountData, Observation, PolicyView, lifecycle};
use crate::{Result, SdkError, instructions::CaseAddress};

/// A validated Case and its related accounts from one observation.
/// Immutable getters prevent consumers from mutating the validated projection.
#[derive(Debug)]
pub struct CaseView {
    pub(super) observation: Observation,
    pub(super) address: CaseAddress,
    pub(super) case: Case,
    pub(super) author_receipt: StakeReceipt,
    policy: PolicyView,
    vault_balance: u64,
}

impl CaseView {
    /// Read protocol state without requiring the opaque subject account to exist.
    /// An explicitly supplied subject is checked for its key, slot and nonexecutable
    /// flag; its owner and contents remain the application's responsibility.
    pub fn read<'subject>(
        observation: Observation,
        expected: CaseAddress,
        case_account: AccountData<'_>,
        policy_account: AccountData<'_>,
        subject_account: impl Into<Option<AccountData<'subject>>>,
        vault_account: AccountData<'_>,
        author_receipt_account: AccountData<'_>,
    ) -> Result<Self> {
        let subject_account = subject_account.into();
        let case: Case = case_account.decode(observation, tekmerion::ID, Case::SPACE)?;
        let policy = PolicyView::read(observation, expected.policy, policy_account)?;
        let (case_key, case_bump) =
            Case::address(&expected.policy, &expected.subject, expected.nonce);
        let (vault_key, vault_bump) = Case::vault_address(&case_key);
        let author_receipt: StakeReceipt =
            author_receipt_account.decode(observation, tekmerion::ID, StakeReceipt::SPACE)?;
        let (author_receipt_key, author_receipt_bump) =
            StakeReceipt::address(&case_key, &case.author);
        if case_account.key != case_key
            || case.policy != expected.policy
            || case.subject != expected.subject
            || case.nonce != expected.nonce
            || case.bump != case_bump
            || case.vault_bump != vault_bump
            || subject_account.is_some_and(|subject| subject.key != expected.subject)
            || vault_account.key != vault_key
            || author_receipt_account.key != author_receipt_key
            || author_receipt.case != case_key
            || author_receipt.owner != case.author
            || author_receipt.bump != author_receipt_bump
        {
            return Err(SdkError::InvalidRelationship);
        }
        if author_receipt.side != AUTHOR_SIDE
            || author_receipt.principal != case.author_principal
            || author_receipt.principal == 0
        {
            return Err(SdkError::InvalidState);
        }
        // The configured client owns its subject schema and current-Case rules.
        // Tekmerion binds only the subject identity, not its owner or contents.
        if let Some(subject) = subject_account {
            if subject.slot != observation.slot {
                return Err(SdkError::InconsistentObservation);
            }
            if subject.executable {
                return Err(SdkError::InvalidAccount);
            }
        }
        let vault: TokenAccount =
            vault_account.decode(observation, token::ID, TokenAccount::LEN)?;
        if vault.mint != tekmerion::USDC_MINT
            || vault.owner != case_key
            || vault.state != token::spl_token::state::AccountState::Initialized
            || vault.is_native.is_some()
            || vault.delegate.is_some()
            || vault.close_authority.is_some()
        {
            return Err(SdkError::InvalidAccount);
        }
        let liability = lifecycle::validate(&case, policy.policy(), author_receipt.claimed)?;
        if u128::from(vault.amount) < liability {
            return Err(SdkError::InsufficientBacking);
        }
        Ok(Self {
            observation,
            address: expected,
            case,
            author_receipt,
            policy,
            vault_balance: vault.amount,
        })
    }

    pub fn case(&self) -> &Case {
        &self.case
    }
    pub fn author_receipt(&self) -> &StakeReceipt {
        &self.author_receipt
    }
    pub fn policy(&self) -> &Policy {
        self.policy.policy()
    }
    pub fn observation(&self) -> Observation {
        self.observation
    }
    pub fn address(&self) -> CaseAddress {
        self.address
    }
    pub fn vault_balance(&self) -> u64 {
        self.vault_balance
    }

    /// Observed cap only. Concurrent funding may reduce actual accepted credit;
    /// the program rechecks the source, time and capacity at execution.
    pub fn quote_contribution(&self, requested: u64) -> Result<u64> {
        if self.case.phase() != tekmerion::CasePhase::Funding || requested == 0 {
            return Err(SdkError::ActionUnavailable);
        }
        FundingSnapshot::new(
            UsdcAmount::from_base_units(self.case.challenge_target),
            UsdcAmount::from_base_units(self.case.challenge_principal),
            ChainDeadline::from_unix_seconds(self.case.funding_deadline),
        )
        .map_err(|_| SdkError::InvalidState)?
        .quote_contribution(
            UsdcAmount::from_base_units(requested),
            ChainTimestamp::from_unix_seconds(self.observation.unix_timestamp),
        )
        .map(|quote| quote.accepted().base_units())
        .map_err(|_| SdkError::ActionUnavailable)
    }
}
