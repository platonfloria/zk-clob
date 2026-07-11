use std::collections::BTreeMap;

use crate::{
    Account, AccountId, BatchInput, BatchOutput, Order, SettlementError,
    hashing::compute_state_root,
    matching::match_and_settle,
    validation::{validate_accounts, validate_config, validate_limits, validate_orders},
};

fn consume_nonces(accounts: &mut Vec<Account>, orders: &[Order]) -> Result<(), SettlementError> {
    let mut order_counts: BTreeMap<AccountId, u64> = BTreeMap::new();
    for order in orders {
        let count = order_counts.entry(*order.trader()).or_default();
        *count = count.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
    }

    let mut next_nonces = BTreeMap::new();
    for account in accounts.iter() {
        let order_count = order_counts.get(account.id()).copied().unwrap_or(0);
        let next_nonce = account
            .next_nonce()
            .checked_add(order_count)
            .ok_or(SettlementError::NonceOverflow)?;
        next_nonces.insert(*account.id(), next_nonce);
    }

    for account in accounts {
        account.set_next_nonce(next_nonces[account.id()]);
    }
    Ok(())
}

pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    validate_limits(&input)?;
    let config = input.config;
    let mut accounts = input.accounts;
    let orders = input.orders;

    validate_config(&config, &accounts)?;
    validate_accounts(&accounts)?;
    validate_orders(&orders, &accounts, &config)?;

    let computed_old_root = compute_state_root(&accounts);
    if computed_old_root != input.expected_old_state_root {
        return Err(SettlementError::OldStateRootMismatch);
    }

    consume_nonces(&mut accounts, &orders)?;

    let trades = match_and_settle(&mut accounts, &orders, &config)?;

    todo!("batch settlement is not implemented yet")
}
