use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetConfig, ExchangeConfig, FeeConfig, MarketConfig, MarketId, Order,
    SettlementError, Side, Trade, consts::BPS_DENOMINATOR,
};

struct WorkingOrder<'a> {
    order: &'a Order,
    remaining: u128,
}

fn account_mut<'a>(
    accounts: &'a mut [Account],
    id: &AccountId,
) -> Result<&'a mut Account, SettlementError> {
    let index = accounts
        .binary_search_by(|account| account.id().cmp(id))
        .map_err(|_| SettlementError::UnknownAccount)?;

    Ok(&mut accounts[index])
}

fn settle_trade(
    accounts: &mut [Account],
    market: &MarketConfig,
    base_asset: &AssetConfig,
    fee_config: &FeeConfig,
    buy: &Order,
    sell: &Order,
    quantity: u128,
    price: u128,
) -> Result<Trade, SettlementError> {
    if buy.trader() == sell.trader() {
        return Err(SettlementError::SelfTrade);
    }

    let base_asset_id = *market.base_asset();
    let quote_asset = *market.quote_asset();
    let quote_amount = quantity
        .checked_mul(price)
        .ok_or(SettlementError::ArithmeticOverflow)?
        / base_asset.scale();
    if quote_amount == 0 {
        return Err(SettlementError::TradeValueRoundsToZero);
    }
    let quote_fee = quote_amount
        .checked_mul(fee_config.buyer_fee_bps() as u128)
        .ok_or(SettlementError::ArithmeticOverflow)?
        / BPS_DENOMINATOR;
    let buyer_debit = quote_amount
        .checked_add(quote_fee)
        .ok_or(SettlementError::ArithmeticOverflow)?;

    // These updates may leave this in-memory slice partially mutated if a later
    // operation fails. This is safe because `settle_batch` owns the account state
    // and discards it on any error, so no partial state or proof can be produced.
    account_mut(accounts, buy.trader())?.debit(quote_asset, buyer_debit)?;
    account_mut(accounts, buy.trader())?.credit(base_asset_id, quantity)?;
    account_mut(accounts, sell.trader())?.debit(base_asset_id, quantity)?;
    account_mut(accounts, sell.trader())?.credit(quote_asset, quote_amount)?;
    account_mut(accounts, fee_config.recipient())?.credit(quote_asset, quote_fee)?;

    Ok(Trade::new(
        *market.id(),
        *buy.trader(),
        *sell.trader(),
        price,
        quantity,
        quote_amount,
        quote_fee,
    ))
}

fn match_market(
    accounts: &mut [Account],
    market: &MarketConfig,
    base_asset: &AssetConfig,
    fee_config: &FeeConfig,
    mut buys: Vec<WorkingOrder>,
    mut sells: Vec<WorkingOrder>,
) -> Result<Vec<Trade>, SettlementError> {
    buys.sort_unstable_by(|a, b| {
        b.order
            .price()
            .cmp(&a.order.price())
            .then_with(|| a.order.sequence().cmp(&b.order.sequence()))
    });
    sells.sort_unstable_by(|a, b| {
        a.order
            .price()
            .cmp(&b.order.price())
            .then_with(|| a.order.sequence().cmp(&b.order.sequence()))
    });

    let mut trades = Vec::new();
    let (mut buy_index, mut sell_index) = (0, 0);
    while buy_index < buys.len() && sell_index < sells.len() {
        let buy = &mut buys[buy_index];
        let sell = &mut sells[sell_index];
        if buy.order.price() < sell.order.price() {
            break;
        }

        let quantity = buy.remaining.min(sell.remaining);
        let price = if buy.order.sequence() < sell.order.sequence() {
            buy.order.price()
        } else {
            sell.order.price()
        };
        trades.push(settle_trade(
            accounts,
            market,
            base_asset,
            fee_config,
            &buy.order,
            &sell.order,
            quantity,
            price,
        )?);

        buy.remaining -= quantity;
        sell.remaining -= quantity;
        if buy.remaining == 0 {
            buy_index += 1;
        }
        if sell.remaining == 0 {
            sell_index += 1;
        }
    }
    Ok(trades)
}

pub fn match_and_settle(
    accounts: &mut [Account],
    orders: &[Order],
    config: &ExchangeConfig,
) -> Result<Vec<Trade>, SettlementError> {
    let mut books: BTreeMap<MarketId, (Vec<WorkingOrder>, Vec<WorkingOrder>)> = BTreeMap::new();
    for order in orders {
        let working = WorkingOrder {
            order,
            remaining: order.quantity(),
        };
        let book = books.entry(*order.market_id()).or_default();
        match order.side() {
            Side::Buy => book.0.push(working),
            Side::Sell => book.1.push(working),
        }
    }

    let fee_config = config.fees();
    let mut trades = Vec::new();
    for (market_id, (buys, sells)) in books {
        let market = config
            .market(&market_id)
            .ok_or(SettlementError::UnknownMarket)?;
        let base_asset = config
            .asset(market.base_asset())
            .ok_or(SettlementError::UnknownAsset)?;
        trades.extend(match_market(
            accounts, market, base_asset, fee_config, buys, sells,
        )?);
    }
    Ok(trades)
}
