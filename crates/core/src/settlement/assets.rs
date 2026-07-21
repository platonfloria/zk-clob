use std::collections::BTreeMap;

use crate::{Account, AssetId, Deposit, SettlementError, SignedWithdrawal};

#[derive(Default)]
pub(super) struct AssetTracker {
    totals: BTreeMap<AssetId, u128>,
}

impl AssetTracker {
    fn add(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        let total = self.totals.entry(asset).or_default();
        *total = total.checked_add(amount).ok_or(SettlementError::ArithmeticOverflow)?;
        Ok(())
    }

    fn subtract(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        let total = self
            .totals
            .get_mut(&asset)
            .ok_or(SettlementError::AssetConservationViolation)?;
        *total = total
            .checked_sub(amount)
            .ok_or(SettlementError::AssetConservationViolation)?;
        if *total == 0 {
            self.totals.remove(&asset);
        }
        Ok(())
    }

    pub(super) fn add_accounts(&mut self, accounts: &[Account]) -> Result<(), SettlementError> {
        for account in accounts {
            for balance in account.balances() {
                self.add(*balance.asset(), balance.available())?;
            }
        }
        Ok(())
    }

    pub(super) fn add_deposits(&mut self, deposits: &[Deposit]) -> Result<(), SettlementError> {
        for deposit in deposits {
            self.add(*deposit.asset(), deposit.amount())?;
        }
        Ok(())
    }

    pub(super) fn subtract_withdrawals(&mut self, withdrawals: &[SignedWithdrawal]) -> Result<(), SettlementError> {
        for withdrawal in withdrawals {
            self.subtract(*withdrawal.asset(), withdrawal.amount())?;
        }
        Ok(())
    }

    pub(super) fn subtract_accounts(&mut self, accounts: &[Account]) -> Result<(), SettlementError> {
        for account in accounts {
            for balance in account.balances() {
                self.subtract(*balance.asset(), balance.available())?;
            }
        }
        Ok(())
    }

    pub(super) fn is_empty(&self) -> bool {
        self.totals.is_empty()
    }
}
