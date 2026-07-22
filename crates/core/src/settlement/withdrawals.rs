use crate::{Account, SettlementError, SignedWithdrawal};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_withdrawals(
    accounts: &mut [Account],
    withdrawals: &[SignedWithdrawal],
) -> Result<(), SettlementError> {
    for withdrawal in withdrawals {
        let index = withdrawal
            .account_index()
            .expect("account index already resolved and checked by validate_withdrawals");
        accounts[index as usize].debit(*withdrawal.asset(), withdrawal.amount())?;
    }
    Ok(())
}
