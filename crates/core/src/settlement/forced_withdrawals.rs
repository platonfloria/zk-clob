use crate::{Account, ForcedWithdrawal, SettlementError};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_forced_withdrawals(
    accounts: &mut [Account],
    requests: &mut [ForcedWithdrawal],
) -> Result<(), SettlementError> {
    for request in requests {
        let drained = match accounts.binary_search_by(|account| account.id().cmp(request.account())) {
            Ok(index) => {
                let available = accounts[index].balance(request.asset());
                let drained = request.amount().min(available);
                if drained > 0 {
                    accounts[index].debit(*request.asset(), drained)?;
                }
                drained
            }
            Err(_) => 0,
        };
        request.set_amount(drained);
    }
    Ok(())
}
