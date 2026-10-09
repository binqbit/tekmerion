use anchor_lang::AccountDeserialize;
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_program_pack::Pack;
use spl_token_interface::state::Account as TokenAccount;
use tekmerion_sdk::accounts::{AccountData, CaseView, Observation};

use super::{Actor, TestContext, pk};

/// The source is one isolated bank observation, with no network finality claim.
pub fn observed_account(address: Address, account: &Account, slot: u64) -> AccountData<'_> {
    AccountData {
        key: pk(address),
        owner: pk(account.owner),
        executable: account.executable,
        data: &account.data,
        slot,
    }
}

impl TestContext {
    pub fn view(&self) -> CaseView {
        let clock: Clock = self.clock();
        let [case, policy, vault, author_receipt] =
            [self.case, self.policy, self.vault, self.author_receipt()]
                .map(|address| self.account(address));
        let subject = self.get_account(&self.subject);
        CaseView::read(
            Observation {
                slot: clock.slot,
                unix_timestamp: clock.unix_timestamp,
            },
            self.case_address(),
            observed_account(self.case, &case, clock.slot),
            observed_account(self.policy, &policy, clock.slot),
            subject
                .as_ref()
                .map(|account| observed_account(self.subject, account, clock.slot)),
            observed_account(self.vault, &vault, clock.slot),
            observed_account(self.author_receipt(), &author_receipt, clock.slot),
        )
        .expect("compatible canonical local observations")
    }

    pub fn account(&self, address: Address) -> Account {
        self.get_account(&address).unwrap()
    }

    pub fn state<T: AccountDeserialize>(&self, address: Address) -> T {
        T::try_deserialize(&mut self.account(address).data.as_slice())
            .expect("canonical Anchor account")
    }

    pub fn policy_state(&self) -> tekmerion::Policy {
        self.state(self.policy)
    }

    pub fn case_state(&self) -> tekmerion::Case {
        self.state(self.case)
    }

    pub fn receipt_state(&self, actor: Actor) -> tekmerion::StakeReceipt {
        self.state(self.receipt(actor))
    }

    pub fn author_receipt_state(&self) -> tekmerion::StakeReceipt {
        self.receipt_state(Actor::Author)
    }

    pub fn unpaid(&self) -> u128 {
        self.case_state()
            .unpaid(self.author_receipt_state().claimed)
    }

    pub fn amount(&self, address: Address) -> u64 {
        TokenAccount::unpack(&self.account(address).data)
            .unwrap()
            .amount
    }

    /// Serialize every Case field with only the two mutable claim-progress fields normalized.
    /// This compares settlement/history invariants while claims change their paid markers.
    pub fn case_without_claim_progress(&self) -> Vec<u8> {
        let mut state = self.case_state();
        state.contributor_unpaid = state.contributor_entitlement;
        state.unclaimed_contributors = 0;
        let mut bytes = Vec::new();
        anchor_lang::AccountSerialize::try_serialize(&state, &mut bytes).unwrap();
        bytes
    }
}
