use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetId, BatchHash, BatchInput, BatchMetadata, BatchOutput, ConfigHash,
    Order, PublicOutput, SettlementError, StateRoot, Trade,
    hashing::DomainSha256Hash as _,
    matching::match_and_settle,
    state::compute_state_root_from_proof,
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
    let trades_hash = trades.hash();

    cycle_tracker!["output-construction", {
        let public = PublicOutput::new(
            metadata,
            old_state_root,
            new_state_root,
            config_hash,
            batch_hash,
            trades_hash,
        );
        BatchOutput::new(public, accounts, trades)
    }]
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
    cycle_tracker![
        "validation",
        validate_limits(&input)?;
        let (
            metadata,
            expected_old_state_root,
            mut accounts,
            state_multiproof,
            orders,
            order_books,
            config,
        ) = (
            input.metadata,
            input.expected_old_state_root,
            input.accounts,
            input.state_multiproof,
            input.orders,
            input.order_books,
            input.config,
        );

        validate_config(&config, &accounts)?;
        validate_accounts(&accounts)?;
        validate_orders(&orders, &accounts, &config)?;
    ];

    cycle_tracker![
        "input-hashing",
        let old_state_root = compute_state_root_from_proof(&accounts, &state_multiproof)?;
        if old_state_root != expected_old_state_root {
            return Err(SettlementError::OldStateRootMismatch);
        }
        let config_hash = config.hash();
        let batch_hash = (
            &metadata,
            &old_state_root,
            &config_hash,
            orders.as_slice(),
        )
            .hash();
    ];

    cycle_tracker![
        "prepare-settlement",
        let old_asset_totals = compute_asset_totals(&accounts)?;
        consume_nonces(&mut accounts, &orders)?;
        let books = build_validated_books(&orders, &order_books, &config)?;
    ];

    let trades = match_and_settle(&mut accounts, books, &config)?;

    Ok(cycle_tracker!["finalize-settlement", {
        cycle_tracker![
            "asset-conservation",
            let new_asset_totals = compute_asset_totals(&accounts)?;
            if old_asset_totals != new_asset_totals {
                return Err(SettlementError::AssetConservationViolation);
            }
        ];

        let new_state_root = compute_state_root_from_proof(&accounts, &state_multiproof)?;

        build_output(
            metadata,
            config_hash,
            batch_hash,
            old_state_root,
            new_state_root,
            accounts,
            trades,
        )
    }])
}
