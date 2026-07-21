use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetId, BatchHash, BatchInput, BatchMetadata, BatchOutput, ConfigHash,
    Deposit, MAX_TOUCHED_ACCOUNTS_PER_BATCH, PublicOutput, SequencedOrder, SettlementError,
    StateRoot, Trade,
    hashing::DomainSha256Hash as _,
    matching::match_and_settle,
    validation::{
        build_validated_books, validate_accounts, validate_config, validate_deposits,
        validate_limits, validate_orders,
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
    old_deposit_cursor: u64,
    new_deposit_cursor: u64,
    deposits: &[Deposit],
) -> BatchOutput {
    let trades_hash = trades.hash();
    let consumed_deposits_hash = deposits.hash();

    cycle_tracker!["output-construction", {
        let public = PublicOutput::new(
            metadata,
            old_state_root,
            new_state_root,
            config_hash,
            batch_hash,
            trades_hash,
            old_deposit_cursor,
            new_deposit_cursor,
            consumed_deposits_hash,
        );
        BatchOutput::new(public, accounts, trades)
    }]
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn apply_deposits(
    accounts: &mut Vec<Account>,
    deposits: &[Deposit],
) -> Result<(), SettlementError> {
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

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn consume_nonces(
    accounts: &mut Vec<Account>,
    orders: &[SequencedOrder],
) -> Result<(), SettlementError> {
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
            mut state,
            old_deposit_cursor,
            deposits,
            orders,
            order_books,
            config,
        ) = (
            input.metadata,
            input.expected_old_state_root,
            input.state,
            input.old_deposit_cursor,
            input.deposits,
            input.orders,
            input.order_books,
            input.config,
        );

        validate_config(&config, state.accounts())?;
        validate_accounts(state.accounts())?;
        let new_deposit_cursor = validate_deposits(&deposits, old_deposit_cursor, &config)?;
    ];

    cycle_tracker![
        "input-hashing",
        let old_state_root = state.root()?;
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
        apply_deposits(state.accounts_mut(), &deposits)?;
        validate_accounts(state.accounts())?;
        validate_orders(&orders, state.accounts(), &config)?;
        let old_asset_totals = compute_asset_totals(state.accounts())?;
        consume_nonces(state.accounts_mut(), &orders)?;
        let books = build_validated_books(&orders, &order_books, &config)?;
    ];

    let trades = match_and_settle(state.accounts_mut(), books, &config)?;

    Ok(cycle_tracker!["finalize-settlement", {
        cycle_tracker![
            "asset-conservation",
            let new_asset_totals = compute_asset_totals(state.accounts())?;
            if old_asset_totals != new_asset_totals {
                return Err(SettlementError::AssetConservationViolation);
            }
        ];

        let new_state_root = state.root()?;

        build_output(
            metadata,
            config_hash,
            batch_hash,
            old_state_root,
            new_state_root,
            state.into_accounts(),
            trades,
            old_deposit_cursor,
            new_deposit_cursor,
            &deposits,
        )
    }])
}
