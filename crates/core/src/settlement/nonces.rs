use crate::{Account, SequencedOrder, SettlementError, SignedWithdrawal};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn consume_nonces(
    accounts: &mut Vec<Account>,
    orders: &[SequencedOrder],
    withdrawals: &[SignedWithdrawal],
) -> Result<(), SettlementError> {
    let mut operation_counts = vec![0u64; accounts.len()];
    for order in orders {
        let index = order
            .account_index()
            .expect("account index already resolved and checked by validate_orders") as usize;
        operation_counts[index] = operation_counts[index]
            .checked_add(1)
            .ok_or(SettlementError::NonceOverflow)?;
    }
    for withdrawal in withdrawals {
        let index = withdrawal
            .account_index()
            .expect("account index already resolved and checked by validate_withdrawals") as usize;
        operation_counts[index] = operation_counts[index]
            .checked_add(1)
            .ok_or(SettlementError::NonceOverflow)?;
    }

    for (account, count) in accounts.iter_mut().zip(operation_counts) {
        let next_nonce = account
            .next_nonce()
            .checked_add(count)
            .ok_or(SettlementError::NonceOverflow)?;
        account.set_next_nonce(next_nonce);
    }
    Ok(())
}
