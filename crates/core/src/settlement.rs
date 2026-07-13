use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetId, BatchHash, BatchInput, BatchMetadata, BatchOutput, ConfigHash,
    Order, PublicOutput, SettlementError, StateRoot, Trade,
    hashing::{compute_batch_hash, compute_config_hash, compute_state_root, compute_trades_hash},
    matching::match_and_settle,
    validation::{
        build_validated_books, validate_accounts, validate_config, validate_limits, validate_orders,
    },
};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
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

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn build_output(
    metadata: BatchMetadata,
    config_hash: ConfigHash,
    batch_hash: BatchHash,
    old_state_root: StateRoot,
    new_state_root: StateRoot,
    accounts: Vec<Account>,
    trades: Vec<Trade>,
) -> BatchOutput {
    let trades_hash = compute_trades_hash(&trades);

    cycle_tracker_start!("output-construction");
    let public = PublicOutput::new(
        metadata,
        old_state_root,
        new_state_root,
        config_hash,
        batch_hash,
        trades_hash,
    );
    let output = BatchOutput::new(public, accounts, trades);
    cycle_tracker_end!("output-construction");
    output
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
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

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    cycle_tracker_start!("validation");
    validate_limits(&input)?;
    let (metadata, expected_old_state_root, mut accounts, orders, order_books, config) = (
        input.metadata,
        input.expected_old_state_root,
        input.accounts,
        input.orders,
        input.order_books,
        input.config,
    );

    validate_config(&config, &accounts)?;
    validate_accounts(&accounts)?;
    validate_orders(&orders, &accounts, &config)?;
    cycle_tracker_end!("validation");

    cycle_tracker_start!("input-hashing");
    let old_state_root = compute_state_root(&accounts);
    if old_state_root != expected_old_state_root {
        return Err(SettlementError::OldStateRootMismatch);
    }
    let config_hash = compute_config_hash(&config);
    let batch_hash = compute_batch_hash(&metadata, &old_state_root, &config_hash, &orders);
    cycle_tracker_end!("input-hashing");

    cycle_tracker_start!("prepare-settlement");
    let old_asset_totals = compute_asset_totals(&accounts)?;
    consume_nonces(&mut accounts, &orders)?;
    let books = build_validated_books(&orders, &order_books, &config)?;
    cycle_tracker_end!("prepare-settlement");

    let trades = match_and_settle(&mut accounts, books, &config)?;

    cycle_tracker_start!("finalize-settlement");
    cycle_tracker_start!("asset-conservation");
    let new_asset_totals = compute_asset_totals(&accounts)?;
    if old_asset_totals != new_asset_totals {
        return Err(SettlementError::AssetConservationViolation);
    }
    cycle_tracker_end!("asset-conservation");

    let new_state_root = compute_state_root(&accounts);

    let output = build_output(
        metadata,
        config_hash,
        batch_hash,
        old_state_root,
        new_state_root,
        accounts,
        trades,
    );
    cycle_tracker_end!("finalize-settlement");

    Ok(output)
}
