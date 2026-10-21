//! Explicit scenario transitions executed through genuine program instructions.
use solana_clock::Clock;

use super::{Actor, BOND, MAIN, TestContext, pda, subject_address};

impl TestContext {
    pub fn set_time(&mut self, timestamp: i64) {
        let mut clock: Clock = self.clock();
        clock.unix_timestamp = timestamp;
        clock.slot += 1;
        self.set_clock(&clock);
    }

    pub fn create_case(&mut self) {
        self.ok(&[self.submit_case_ix(BOND)], &self.creation_signers());
    }

    pub fn select_fresh_subject(&mut self, identifier: u64) {
        self.subject = subject_address(identifier);
        self.fund(self.subject, 1_000_000);
        self.case = pda(
            &[
                b"case",
                self.policy.as_ref(),
                self.subject.as_ref(),
                self.actor(Actor::Author).as_ref(),
                &0_u64.to_le_bytes(),
            ],
            MAIN,
        );
        self.vault = pda(&[b"vault", self.case.as_ref()], MAIN);
        self.protected
            .extend([self.subject, self.case, self.vault, self.author_receipt()]);
    }

    pub fn contribute(&mut self, actor: Actor, amount: u64) {
        self.ok(&[self.contribute_ix(actor, amount)], &[actor]);
        self.protected.insert(self.receipt(actor));
    }

    /// Set the local Clock to this Case's funding deadline, then finalize it optimistically.
    /// This deliberately sets an exact boundary; it does not promise monotonic time.
    pub fn finalize_optimistic_at_funding_deadline(&mut self) {
        self.set_time(self.case_state().funding_deadline);
        self.ok(&[self.finalize_ix()], &[]);
    }
}
