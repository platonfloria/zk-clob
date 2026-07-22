use crate::{Account, ForcedWithdrawal, SettlementError};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_forced_withdrawals(
    accounts: &mut [Account],
    requests: &mut [ForcedWithdrawal],
) -> Result<(), SettlementError> {
    for request in requests {
        let drained = match request.account_index() {
            Some(index)
                if accounts
                    .get(index as usize)
                    .is_some_and(|account| account.id() == request.account()) =>
            {
                let index = index as usize;
                let available = accounts[index].balance(request.asset());
                let drained = request.amount().min(available);
                if drained > 0 {
                    accounts[index].debit(*request.asset(), drained)?;
                }
                drained
            }
            _ => 0,
        };
        request.set_amount(drained);
    }
    Ok(())
}
