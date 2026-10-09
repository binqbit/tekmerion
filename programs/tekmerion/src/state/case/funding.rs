use anchor_lang::prelude::*;

use super::{Case, CasePhase};
use crate::calculations::{
    amounts::UsdcAmount,
    funding::{ContributionQuote, FundingSnapshot},
    time::{ChainDeadline, ChainTimestamp},
};
use crate::{
    PolicyRules,
    errors::{TekmerionError, domain_error},
};

impl Case {
    pub(crate) fn funding_snapshot(&self) -> Result<FundingSnapshot> {
        FundingSnapshot::new(
            UsdcAmount::from_base_units(self.challenge_target),
            UsdcAmount::from_base_units(self.challenge_principal),
            ChainDeadline::from_unix_seconds(self.funding_deadline),
        )
        .map_err(domain_error)
    }

    /// Record a current quote only after its exact transfer has succeeded.
    pub(crate) fn record_contribution(
        &mut self,
        quote: ContributionQuote,
        previous_principal: u64,
        rules: &PolicyRules,
        now: ChainTimestamp,
    ) -> Result<()> {
        self.require_funding()?;

        let accepted = quote.accepted().base_units();
        let resulting = quote.resulting_principal().base_units();
        require!(
            accepted > 0
                && self.challenge_principal.checked_add(accepted) == Some(resulting)
                && resulting <= self.challenge_target
                && quote.reaches_target() == (resulting == self.challenge_target),
            TekmerionError::InvalidContribution
        );

        if previous_principal == 0 {
            self.unclaimed_contributors = self
                .unclaimed_contributors
                .checked_add(1)
                .ok_or(TekmerionError::ArithmeticOverflow)?;
        }

        if quote.reaches_target() {
            let deadline =
                ChainDeadline::checked_after(now, rules.audit_timeout).map_err(domain_error)?;

            self.audit.start(deadline.unix_seconds())?;
            self.phase = CasePhase::Funded;
        }

        self.challenge_principal = resulting;

        Ok(())
    }
}
