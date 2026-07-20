use std::collections::{BTreeMap, BTreeSet};

use zk_clob_core::{
    AccountId, BatchInput, BatchMetadata, ExchangeConfig, MAX_ORDERS_PER_BATCH,
    MAX_TOUCHED_ACCOUNTS_PER_BATCH, MarketId, MarketOrderBook, Order, Side,
};

use crate::{AccountTree, BatchBuildError};

pub struct BatchBuilder<'a> {
    state: &'a AccountTree,
    config: &'a ExchangeConfig,
    metadata: BatchMetadata,
    orders: Vec<Order>,
    touched_accounts: BTreeSet<AccountId>,
    sequences: BTreeSet<u64>,
    nonces: BTreeMap<AccountId, BTreeSet<u64>>,
    books: BTreeMap<MarketId, (Vec<u32>, Vec<u32>)>,
}

impl<'a> BatchBuilder<'a> {
    pub fn new(
        state: &'a AccountTree,
        config: &'a ExchangeConfig,
        metadata: BatchMetadata,
    ) -> Self {
        let mut touched_accounts = BTreeSet::new();
        touched_accounts.insert(*config.fees().recipient());
        Self {
            state,
            config,
            metadata,
            orders: Vec::new(),
            touched_accounts,
            sequences: BTreeSet::new(),
            nonces: BTreeMap::new(),
            books: BTreeMap::new(),
        }
    }

    pub fn order(&mut self, order: Order) -> Result<(), BatchBuildError> {
        if self.orders.len() >= MAX_ORDERS_PER_BATCH {
            return Err(BatchBuildError::TooManyOrders);
        }
        if order.price() == 0 {
            return Err(BatchBuildError::ZeroPrice);
        }
        if order.quantity() == 0 {
            return Err(BatchBuildError::ZeroQuantity);
        }
        if self.state.account(order.trader()).is_none() {
            return Err(BatchBuildError::UnknownAccount(*order.trader()));
        }
        if self
            .config
            .markets()
            .iter()
            .all(|market| market.id() != order.market_id())
        {
            return Err(BatchBuildError::UnknownMarket(*order.market_id()));
        }
        if self.sequences.contains(&order.sequence()) {
            return Err(BatchBuildError::DuplicateSequence(order.sequence()));
        }
        let account = self
            .state
            .account(order.trader())
            .ok_or(BatchBuildError::UnknownAccount(*order.trader()))?;
        if order.nonce() < account.next_nonce() {
            return Err(BatchBuildError::InvalidNonce(*order.trader()));
        }
        if self
            .nonces
            .get(order.trader())
            .is_some_and(|nonces| nonces.contains(&order.nonce()))
        {
            return Err(BatchBuildError::DuplicateNonce(
                *order.trader(),
                order.nonce(),
            ));
        }
        if !self.touched_accounts.contains(order.trader())
            && self.touched_accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH
        {
            return Err(BatchBuildError::TooManyAccounts);
        }

        let index =
            u32::try_from(self.orders.len()).map_err(|_| BatchBuildError::OrderIndexOverflow)?;
        self.sequences.insert(order.sequence());
        self.nonces
            .entry(*order.trader())
            .or_default()
            .insert(order.nonce());
        self.touched_accounts.insert(*order.trader());
        let (buys, sells) = self.books.entry(*order.market_id()).or_default();
        match order.side() {
            Side::Buy => buys.push(index),
            Side::Sell => sells.push(index),
        }
        self.orders.push(order);
        Ok(())
    }

    pub fn build(self) -> Result<BatchInput, BatchBuildError> {
        for (account_id, nonces) in &self.nonces {
            let mut expected = self
                .state
                .account(account_id)
                .ok_or(BatchBuildError::UnknownAccount(*account_id))?
                .next_nonce();
            for nonce in nonces {
                if *nonce != expected {
                    return Err(BatchBuildError::InvalidNonce(*account_id));
                }
                expected = expected
                    .checked_add(1)
                    .ok_or(BatchBuildError::InvalidNonce(*account_id))?;
            }
        }

        let state_witness = self.state.witness(&self.touched_accounts)?;
        let order_books = self
            .books
            .into_iter()
            .map(|(market_id, (mut buys, mut sells))| {
                buys.sort_unstable_by(|left, right| {
                    Side::Buy.compare_priority(
                        &self.orders[*left as usize],
                        &self.orders[*right as usize],
                    )
                });
                sells.sort_unstable_by(|left, right| {
                    Side::Sell.compare_priority(
                        &self.orders[*left as usize],
                        &self.orders[*right as usize],
                    )
                });
                MarketOrderBook::new(market_id, buys, sells)
            })
            .collect();

        Ok(BatchInput::new(
            self.metadata.protocolVersion,
            self.metadata.chainId,
            self.metadata.exchangeId,
            self.metadata.batchId,
            self.state.root(),
            state_witness,
            self.orders,
            order_books,
            self.config.clone(),
        ))
    }
}
