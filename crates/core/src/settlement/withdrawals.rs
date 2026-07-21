use crate::{Account, SettlementError, SignedWithdrawal};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_withdrawals(
    accounts: &mut [Account],
    withdrawals: &[SignedWithdrawal],
) -> Result<(), SettlementError> {
    for withdrawal in withdrawals {
        let account = accounts
            .binary_search_by(|account| account.id().cmp(withdrawal.account()))
            .map_err(|_| SettlementError::UnknownAccount)?;
        accounts[account].debit(*withdrawal.asset(), withdrawal.amount())?;
    }
    Ok(())
}
