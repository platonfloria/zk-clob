use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use crate::{
    Account, AssetConfig, BatchInput, ExchangeConfig, MarketConfig, MarketId, MarketOrderBook,
    Order, SettlementError, Side,
};

pub(crate) struct ValidatedMarketBook<'a> {
    pub(crate) market: &'a MarketConfig,
    pub(crate) base_asset: &'a AssetConfig,
    pub(crate) buys: Vec<&'a Order>,
    pub(crate) sells: Vec<&'a Order>,
}

const MAX_ACCOUNTS_PER_BATCH: usize = 1_000;
const MAX_ORDERS_PER_BATCH: usize = 1_000;
const MAX_ASSETS: usize = 1_000;
const MAX_MARKETS: usize = 1_000;
const BPS_DENOMINATOR: u16 = 10_000;

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
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
    if input.order_books.len() > MAX_MARKETS {
        return Err(SettlementError::TooManyMarkets);
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
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
        if !market_ids.insert(market.id()) {
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

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_accounts(accounts: &[Account]) -> Result<(), SettlementError> {
    let mut account_ids = BTreeSet::new();
    for account in accounts {
        if !account_ids.insert(account.id()) {
            return Err(SettlementError::DuplicateAccount);
        }

        let mut balance_assets = BTreeSet::new();
        for balance in account.balances() {
            if balance.available() == 0 {
                return Err(SettlementError::ZeroBalance);
            }
            if !balance_assets.insert(balance.asset()) {
                return Err(SettlementError::DuplicateBalance);
            }
        }
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_orders(
    orders: &[Order],
    accounts: &[Account],
    config: &ExchangeConfig,
) -> Result<(), SettlementError> {
    let account_nonces: BTreeMap<_, _> = accounts
        .iter()
        .map(|account| (account.id(), account.next_nonce()))
        .collect();

    let mut sequences = BTreeSet::new();
    let mut nonces: BTreeMap<_, Vec<_>> = BTreeMap::new();

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
        if config.market(order.market_id()).is_none() {
            return Err(SettlementError::UnknownMarket);
        }
        if !sequences.insert(order.sequence()) {
            return Err(SettlementError::DuplicateSequence);
        }
        nonces
            .entry(order.trader())
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

fn validate_book_orders<'a>(
    market_id: &MarketId,
    indices: &[u32],
    side: Side,
    orders: &'a [Order],
    seen_indices: &mut [bool],
) -> Result<Vec<&'a Order>, SettlementError> {
    let mut validated_orders: Vec<&'a Order> = Vec::with_capacity(indices.len());

    for &index in indices {
        let index = usize::try_from(index).map_err(|_| SettlementError::InvalidOrderIndex)?;
        let order = orders
            .get(index)
            .ok_or(SettlementError::InvalidOrderIndex)?;
        if seen_indices[index] {
            return Err(SettlementError::DuplicateOrderIndex);
        }
        seen_indices[index] = true;

        if order.market_id() != market_id {
            return Err(SettlementError::OrderBookMarketMismatch);
        }
        if order.side() != side {
            return Err(SettlementError::OrderBookSideMismatch);
        }
        if validated_orders
            .last()
            .is_some_and(|previous| side.compare_priority(previous, order) != Ordering::Less)
        {
            return Err(SettlementError::UnsortedOrderBook);
        }
        validated_orders.push(order);
    }

    Ok(validated_orders)
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(crate) fn build_validated_books<'a>(
    orders: &'a [Order],
    order_books: &[MarketOrderBook],
    config: &'a ExchangeConfig,
) -> Result<Vec<ValidatedMarketBook<'a>>, SettlementError> {
    let mut books = Vec::with_capacity(order_books.len());
    let mut previous_market = None;
    let mut seen_indices = vec![false; orders.len()];

    for book in order_books {
        if book.buy_indices().is_empty() && book.sell_indices().is_empty() {
            return Err(SettlementError::EmptyOrderBook);
        }
        if let Some(market) = previous_market {
            if market == *book.market_id() {
                return Err(SettlementError::DuplicateMarketOrderBook);
            }
            if market > *book.market_id() {
                return Err(SettlementError::UnsortedMarketOrderBooks);
            }
        }
        previous_market = Some(*book.market_id());

        let market = config
            .market(book.market_id())
            .ok_or(SettlementError::UnknownMarket)?;
        let base_asset = config
            .asset(market.base_asset())
            .ok_or(SettlementError::UnknownAsset)?;
        let buys = validate_book_orders(
            book.market_id(),
            book.buy_indices(),
            Side::Buy,
            orders,
            &mut seen_indices,
        )?;
        let sells = validate_book_orders(
            book.market_id(),
            book.sell_indices(),
            Side::Sell,
            orders,
            &mut seen_indices,
        )?;
        books.push(ValidatedMarketBook {
            market,
            base_asset,
            buys,
            sells,
        });
    }

    if seen_indices.into_iter().any(|seen| !seen) {
        return Err(SettlementError::MissingOrderIndex);
    }
    Ok(books)
}
