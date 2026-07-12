use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetId, BatchInput, BatchMetadata, BatchOutput, ExchangeConfig, Order,
    PublicOutput, SettlementError, StateRoot, Trade,
    hashing::{compute_batch_hash, compute_config_hash, compute_state_root, compute_trades_hash},
    matching::match_and_settle,
    validation::{validate_accounts, validate_config, validate_limits, validate_orders},
};

fn compute_asset_totals(accounts: &[Account]) -> Result<BTreeMap<AssetId, u128>, SettlementError> {
    let mut totals = BTreeMap::new();
    for account in accounts {
        for balance in account.balances() {
            let total = totals.entry(*balance.asset()).or_insert(0u128);
            *total = total
                .checked_add(balance.available())
                .ok_or(SettlementError::ArithmeticOverflow)?;
        }
    }
    Ok(totals)
}

fn build_output(
    metadata: BatchMetadata,
    config: &ExchangeConfig,
    orders: &[Order],
    old_state_root: StateRoot,
    new_state_root: StateRoot,
    accounts: Vec<Account>,
    trades: Vec<Trade>,
) -> BatchOutput {
    let config_hash = compute_config_hash(config);
    let batch_hash = compute_batch_hash(&metadata, &old_state_root, &config_hash, orders);
    let trades_hash = compute_trades_hash(&trades);

    let public = PublicOutput::new(
        metadata,
        old_state_root,
        new_state_root,
        config_hash,
        batch_hash,
        trades_hash,
    );
    BatchOutput::new(public, accounts, trades)
}

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
    let (metadata, expected_old_state_root, mut accounts, orders, config) = (
        input.metadata,
        input.expected_old_state_root,
        input.accounts,
        input.orders,
        input.config,
    );

    validate_config(&config, &accounts)?;
    validate_accounts(&accounts)?;
    validate_orders(&orders, &accounts, &config)?;

    let old_state_root = compute_state_root(&accounts);
    if old_state_root != expected_old_state_root {
        return Err(SettlementError::OldStateRootMismatch);
    }

    let old_asset_totals = compute_asset_totals(&accounts)?;
    consume_nonces(&mut accounts, &orders)?;
    let trades = match_and_settle(&mut accounts, &orders, &config)?;
    let new_asset_totals = compute_asset_totals(&accounts)?;
    if old_asset_totals != new_asset_totals {
        return Err(SettlementError::AssetConservationViolation);
    }

    let new_state_root = compute_state_root(&accounts);

    Ok(build_output(
        metadata,
        &config,
        &orders,
        old_state_root,
        new_state_root,
        accounts,
        trades,
    ))
}
