use crate::{Account, Deposit, MAX_TOUCHED_ACCOUNTS_PER_BATCH, SettlementError};

fn locate_deposit_account(accounts: &[Account], deposit: &Deposit) -> Result<usize, usize> {
    let index = deposit
        .account_index()
        .expect("BatchBuilder must attach an account_index to every deposit") as usize;
    let at_index = accounts.get(index);
    if at_index.is_some_and(|account| account.id() == deposit.account()) {
        return Ok(index);
    }
    let before_ok = index == 0 || accounts.get(index - 1).is_some_and(|a| a.id() < deposit.account());
    let after_ok = at_index.is_none_or(|a| deposit.account() < a.id());
    if before_ok && after_ok {
        return Err(index);
    }
    accounts.binary_search_by_key(deposit.account(), |account| *account.id())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn apply_deposits(accounts: &mut Vec<Account>, deposits: &[Deposit]) -> Result<(), SettlementError> {
    for deposit in deposits {
        let index = match locate_deposit_account(accounts, deposit) {
            Ok(index) => index,
            Err(index) => {
                if accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH {
                    return Err(SettlementError::TooManyAccounts);
                }
                let account = Account::new(*deposit.account(), Vec::new(), 0);
                accounts.insert(index, account);
                index
            }
        };
        accounts[index].credit(*deposit.asset(), deposit.amount())?
    }
    Ok(())
}
