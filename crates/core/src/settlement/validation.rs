use std::{cmp::Ordering, collections::BTreeSet};

use crate::{
    Account, AssetConfig, BatchInput, Deposit, ExchangeConfig, MAX_DEPOSITS_PER_BATCH, MAX_ORDERS_PER_BATCH,
    MAX_TOUCHED_ACCOUNTS_PER_BATCH, MAX_WITHDRAWALS_PER_BATCH, MarketConfig, MarketId, MarketOrderBook, SequencedOrder,
    SettlementError, Side, SignedWithdrawal, SigningDomainHash,
};

pub(crate) struct ValidatedMarketBook<'a> {
    pub(crate) market: &'a MarketConfig,
    pub(crate) base_asset: &'a AssetConfig,
    pub(crate) buys: Vec<&'a SequencedOrder>,
    pub(crate) sells: Vec<&'a SequencedOrder>,
}

const MAX_ASSETS: usize = 1_000;
const MAX_MARKETS: usize = 1_000;
const BPS_DENOMINATOR: u16 = 10_000;

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_limits(input: &BatchInput) -> Result<(), SettlementError> {
    if input.state.accounts().len() > MAX_TOUCHED_ACCOUNTS_PER_BATCH {
        return Err(SettlementError::TooManyAccounts);
    }
    if input.orders.len() > MAX_ORDERS_PER_BATCH {
        return Err(SettlementError::TooManyOrders);
    }
    if input.deposits.len() > MAX_DEPOSITS_PER_BATCH {
        return Err(SettlementError::TooManyDeposits);
    }
    if input.withdrawals.len() > MAX_WITHDRAWALS_PER_BATCH {
        return Err(SettlementError::TooManyWithdrawals);
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
pub fn validate_deposits(
    deposits: &[Deposit],
    old_cursor: u64,
    config: &ExchangeConfig,
) -> Result<u64, SettlementError> {
    let mut expected = old_cursor;
    for deposit in deposits {
        if deposit.id() != expected {
            return Err(SettlementError::InvalidDepositCursor);
        }
        if deposit.amount() == 0 {
            return Err(SettlementError::ZeroDepositAmount);
        }
        if config.asset(deposit.asset()).is_none() {
            return Err(SettlementError::UnknownAsset);
        }
        expected = expected.checked_add(1).ok_or(SettlementError::DepositCursorOverflow)?;
    }
    Ok(expected)
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_config(config: &ExchangeConfig, accounts: &[Account]) -> Result<(), SettlementError> {
    let mut previous_asset = None;
    for asset in config.assets() {
        if asset.scale() == 0 {
            return Err(SettlementError::ZeroAssetScale);
        }
        if let Some(previous) = previous_asset {
            if previous == asset.id() {
                return Err(SettlementError::DuplicateAsset);
            }
            if previous > asset.id() {
                return Err(SettlementError::UnsortedAssets);
            }
        }
        previous_asset = Some(asset.id());
    }

    let mut previous_market = None;
    for market in config.markets() {
        if let Some(previous) = previous_market {
            if previous == market.id() {
                return Err(SettlementError::DuplicateMarket);
            }
            if previous > market.id() {
                return Err(SettlementError::UnsortedMarkets);
            }
        }
        previous_market = Some(market.id());
        if market.base_asset() == market.quote_asset() {
            return Err(SettlementError::IdenticalMarketAssets);
        }
        if config.asset(market.base_asset()).is_none() || config.asset(market.quote_asset()).is_none() {
            return Err(SettlementError::UnknownAsset);
        }
    }

    if config.fees().buyer_fee_bps() > BPS_DENOMINATOR {
        return Err(SettlementError::InvalidFee);
    }
    if !accounts.iter().any(|account| account.id() == config.fees().recipient()) {
        return Err(SettlementError::MissingFeeRecipient);
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_accounts(accounts: &[Account]) -> Result<(), SettlementError> {
    if accounts.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
        let mut account_ids = BTreeSet::new();
        for account in accounts {
            if !account_ids.insert(account.id()) {
                return Err(SettlementError::DuplicateAccount);
            }
        }
        return Err(SettlementError::UnsortedAccounts);
    }

    for account in accounts {
        let balances = account.balances();
        for balance in balances {
            if balance.available() == 0 {
                return Err(SettlementError::ZeroBalance);
            }
        }
        if balances.windows(2).any(|pair| pair[0].asset() >= pair[1].asset()) {
            let mut balance_assets = BTreeSet::new();
            for balance in balances {
                if !balance_assets.insert(balance.asset()) {
                    return Err(SettlementError::DuplicateBalance);
                }
            }
            return Err(SettlementError::UnsortedBalances);
        }
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_orders(
    orders: &[SequencedOrder],
    accounts: &[Account],
    config: &ExchangeConfig,
    domain_hash: &SigningDomainHash,
) -> Result<(), SettlementError> {
    let mut sequences = Vec::with_capacity(orders.len());

    cycle_tracker!("scan", {
        for order in orders {
            if order.price() == 0 {
                return Err(SettlementError::ZeroPrice);
            }
            if order.quantity() == 0 {
                return Err(SettlementError::ZeroQuantity);
            }
            if !order.has_valid_signature(domain_hash) {
                return Err(SettlementError::InvalidOrderSignature);
            }
            if accounts
                .binary_search_by(|account| account.id().cmp(order.trader()))
                .is_err()
            {
                return Err(SettlementError::UnknownAccount);
            }
            if config.market(order.market_id()).is_none() {
                return Err(SettlementError::UnknownMarket);
            }
            sequences.push(order.sequence());
        }
    });

    cycle_tracker!("sequences", {
        sequences.sort_unstable();
        if sequences.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SettlementError::DuplicateSequence);
        }
    });

    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_withdrawals(
    withdrawals: &[SignedWithdrawal],
    accounts: &[Account],
    config: &ExchangeConfig,
    domain_hash: &SigningDomainHash,
) -> Result<(), SettlementError> {
    for withdrawal in withdrawals {
        if withdrawal.amount() == 0 {
            return Err(SettlementError::ZeroWithdrawalAmount);
        }
        if !withdrawal.has_valid_signature(domain_hash) {
            return Err(SettlementError::InvalidWithdrawalSignature);
        }
        if accounts
            .binary_search_by(|account| account.id().cmp(withdrawal.account()))
            .is_err()
        {
            return Err(SettlementError::UnknownAccount);
        }
        if config.asset(withdrawal.asset()).is_none() {
            return Err(SettlementError::UnknownAsset);
        }
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn validate_nonces(
    orders: &[SequencedOrder],
    withdrawals: &[SignedWithdrawal],
    accounts: &[Account],
) -> Result<(), SettlementError> {
    let mut nonces = Vec::with_capacity(orders.len() + withdrawals.len());
    for order in orders {
        let account_index = accounts
            .binary_search_by(|account| account.id().cmp(order.trader()))
            .map_err(|_| SettlementError::UnknownAccount)?;
        nonces.push((account_index, order.nonce()));
    }
    for withdrawal in withdrawals {
        let account_index = accounts
            .binary_search_by(|account| account.id().cmp(withdrawal.account()))
            .map_err(|_| SettlementError::UnknownAccount)?;
        nonces.push((account_index, withdrawal.nonce()));
    }

    nonces.sort_unstable();
    for account_nonces in nonces.chunk_by(|left, right| left.0 == right.0) {
        let mut expected = accounts[account_nonces[0].0].next_nonce();
        for &(_, nonce) in account_nonces {
            if nonce != expected {
                return Err(SettlementError::InvalidNonce);
            }
            expected = expected.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
        }
    }
    Ok(())
}

fn validate_book_orders<'a>(
    market_id: &MarketId,
    indices: &[u32],
    side: Side,
    orders: &'a [SequencedOrder],
    seen_indices: &mut [bool],
) -> Result<Vec<&'a SequencedOrder>, SettlementError> {
    let mut validated_orders: Vec<&'a SequencedOrder> = Vec::with_capacity(indices.len());

    for &index in indices {
        let index = usize::try_from(index).map_err(|_| SettlementError::InvalidOrderIndex)?;
        let order = orders.get(index).ok_or(SettlementError::InvalidOrderIndex)?;
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
    orders: &'a [SequencedOrder],
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

        let market = config.market(book.market_id()).ok_or(SettlementError::UnknownMarket)?;
        let base_asset = config.asset(market.base_asset()).ok_or(SettlementError::UnknownAsset)?;
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
