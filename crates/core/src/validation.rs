use std::collections::{BTreeMap, BTreeSet};

use crate::{Account, AccountId, BatchInput, ExchangeConfig, Order, SettlementError};

const MAX_ACCOUNTS_PER_BATCH: usize = 1_000;
const MAX_ORDERS_PER_BATCH: usize = 1_000;
const MAX_ASSETS: usize = 1_000;
const MAX_MARKETS: usize = 1_000;
const BPS_DENOMINATOR: u16 = 10_000;

pub fn validate_limits(input: &BatchInput) -> Result<(), SettlementError> {
    if input.accounts.len() > MAX_ACCOUNTS_PER_BATCH {
        return Err(SettlementError::TooManyAccounts);
    }
    if input.orders.len() > MAX_ORDERS_PER_BATCH {
        return Err(SettlementError::TooManyOrders);
    }
    if input.config.assets().len() > MAX_ASSETS {
        return Err(SettlementError::TooManyAssets);
    }
    if input.config.markets().len() > MAX_MARKETS {
        return Err(SettlementError::TooManyMarkets);
    }
    Ok(())
}

pub fn validate_config(
    config: &ExchangeConfig,
    accounts: &[Account],
) -> Result<(), SettlementError> {
    let mut asset_ids = BTreeSet::new();
    for asset in config.assets() {
        if asset.scale() == 0 {
            return Err(SettlementError::ZeroAssetScale);
        }
        if !asset_ids.insert(asset.id()) {
            return Err(SettlementError::DuplicateAsset);
        }
    }

    let mut market_ids = BTreeSet::new();
    for market in config.markets() {
        if !market_ids.insert(*market.id()) {
            return Err(SettlementError::DuplicateMarket);
        }
        if market.base_asset() == market.quote_asset() {
            return Err(SettlementError::IdenticalMarketAssets);
        }
        if !asset_ids.contains(market.base_asset()) || !asset_ids.contains(market.quote_asset()) {
            return Err(SettlementError::UnknownAsset);
        }
    }

    if config.fees().buyer_fee_bps() > BPS_DENOMINATOR {
        return Err(SettlementError::InvalidFee);
    }
    if !accounts
        .iter()
        .any(|account| account.id() == config.fees().recipient())
    {
        return Err(SettlementError::MissingFeeRecipient);
    }
    Ok(())
}

pub fn validate_accounts(accounts: &[Account]) -> Result<(), SettlementError> {
    let mut account_ids = BTreeSet::new();
    for account in accounts {
        if !account_ids.insert(*account.id()) {
            return Err(SettlementError::DuplicateAccount);
        }

        let mut balance_assets = BTreeSet::new();
        for balance in account.balances() {
            if balance.available() == 0 {
                return Err(SettlementError::ZeroBalance);
            }
            if !balance_assets.insert(*balance.asset()) {
                return Err(SettlementError::DuplicateBalance);
            }
        }
    }
    Ok(())
}

pub fn validate_orders(
    orders: &[Order],
    accounts: &[Account],
    config: &ExchangeConfig,
) -> Result<(), SettlementError> {
    let account_nonces: BTreeMap<_, _> = accounts
        .iter()
        .map(|account| (*account.id(), account.next_nonce()))
        .collect();

    let market_ids: BTreeSet<_> = config.markets().iter().map(|market| *market.id()).collect();
    let mut sequences = BTreeSet::new();
    let mut nonces: BTreeMap<AccountId, Vec<u64>> = BTreeMap::new();

    for order in orders {
        if order.price() == 0 {
            return Err(SettlementError::ZeroPrice);
        }
        if order.quantity() == 0 {
            return Err(SettlementError::ZeroQuantity);
        }
        if !account_nonces.contains_key(order.trader()) {
            return Err(SettlementError::UnknownAccount);
        }
        if !market_ids.contains(order.market_id()) {
            return Err(SettlementError::UnknownMarket);
        }
        if !sequences.insert(order.sequence()) {
            return Err(SettlementError::DuplicateSequence);
        }
        nonces
            .entry(*order.trader())
            .or_default()
            .push(order.nonce());
    }

    for (account, mut order_nonces) in nonces {
        order_nonces.sort_unstable();
        let mut expected = account_nonces[&account];
        for nonce in order_nonces {
            if nonce != expected {
                return Err(SettlementError::InvalidNonce);
            }
            expected = expected
                .checked_add(1)
                .ok_or(SettlementError::NonceOverflow)?;
        }
    }
    Ok(())
}
