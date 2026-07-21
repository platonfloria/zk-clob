use crate::{Account, Deposit, MAX_TOUCHED_ACCOUNTS_PER_BATCH, SettlementError};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_deposits(accounts: &mut Vec<Account>, deposits: &[Deposit]) -> Result<(), SettlementError> {
    for deposit in deposits {
        match accounts.binary_search_by_key(deposit.account(), |account| *account.id()) {
            Ok(index) => accounts[index].credit(*deposit.asset(), deposit.amount())?,
            Err(index) => {
                if accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH {
                    return Err(SettlementError::TooManyAccounts);
                }
                let mut account = Account::new(*deposit.account(), Vec::new(), 0);
                account.credit(*deposit.asset(), deposit.amount())?;
                accounts.insert(index, account);
            }
        }
    }
    Ok(())
}
