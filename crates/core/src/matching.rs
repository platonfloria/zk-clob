use crate::{
    Account, AccountId, AssetConfig, ExchangeConfig, FeeConfig, MarketConfig, SequencedOrder, SettlementError, Trade,
    consts::BPS_DENOMINATOR, validation::ValidatedMarketBook,
};

fn account_mut<'a>(accounts: &'a mut [Account], id: &AccountId) -> Result<&'a mut Account, SettlementError> {
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
    buy: &SequencedOrder,
    sell: &SequencedOrder,
    quantity: u128,
    price: u128,
) -> Result<Trade, SettlementError> {
    if buy.trader() == sell.trader() {
        return Err(SettlementError::SelfTrade);
    }

    let base_asset_id = *market.base_asset();
    let quote_asset = *market.quote_asset();
    let quote_amount = quantity.checked_mul(price).ok_or(SettlementError::ArithmeticOverflow)? / base_asset.scale();
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

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn match_market(
    accounts: &mut [Account],
    market: &MarketConfig,
    base_asset: &AssetConfig,
    fee_config: &FeeConfig,
    buys: Vec<&SequencedOrder>,
    sells: Vec<&SequencedOrder>,
) -> Result<Vec<Trade>, SettlementError> {
    let mut trades = Vec::new();
    let (mut buy_index, mut sell_index) = (0, 0);
    let (mut buy_remaining, mut sell_remaining) = (0, 0);
    while buy_index < buys.len() && sell_index < sells.len() {
        if buy_remaining == 0 {
            buy_remaining = buys[buy_index].quantity();
        }
        if sell_remaining == 0 {
            sell_remaining = sells[sell_index].quantity();
        }

        let buy = buys[buy_index];
        let sell = sells[sell_index];
        if buy.price() < sell.price() {
            break;
        }

        let quantity = buy_remaining.min(sell_remaining);
        let price = if buy.sequence() < sell.sequence() {
            buy.price()
        } else {
            sell.price()
        };
        trades.push(settle_trade(
            accounts, market, base_asset, fee_config, buy, sell, quantity, price,
        )?);

        buy_remaining -= quantity;
        sell_remaining -= quantity;
        if buy_remaining == 0 {
            buy_index += 1;
        }
        if sell_remaining == 0 {
            sell_index += 1;
        }
    }
    Ok(trades)
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn match_and_settle(
    accounts: &mut [Account],
    books: Vec<ValidatedMarketBook<'_>>,
    config: &ExchangeConfig,
) -> Result<Vec<Trade>, SettlementError> {
    let fee_config = config.fees();
    let mut trades = Vec::new();
    for book in books {
        trades.extend(match_market(
            accounts,
            book.market,
            book.base_asset,
            fee_config,
            book.buys,
            book.sells,
        )?);
    }
    Ok(trades)
}
