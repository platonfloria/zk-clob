use std::collections::BTreeMap;

use crate::{Account, AccountId, SequencedOrder, SettlementError, SignedWithdrawal};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn consume_nonces(
    accounts: &mut Vec<Account>,
    orders: &[SequencedOrder],
    withdrawals: &[SignedWithdrawal],
) -> Result<(), SettlementError> {
    let mut operation_counts: BTreeMap<AccountId, u64> = BTreeMap::new();
    for order in orders {
        let count = operation_counts.entry(*order.trader()).or_default();
        *count = count.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
    }
    for withdrawal in withdrawals {
        let count = operation_counts.entry(*withdrawal.account()).or_default();
        *count = count.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
    }

    let mut next_nonces = BTreeMap::new();
    for account in accounts.iter() {
        let operation_count = operation_counts.get(account.id()).copied().unwrap_or(0);
        let next_nonce = account
            .next_nonce()
            .checked_add(operation_count)
            .ok_or(SettlementError::NonceOverflow)?;
        next_nonces.insert(*account.id(), next_nonce);
    }

    for account in accounts {
        account.set_next_nonce(next_nonces[account.id()]);
    }
    Ok(())
}
