use std::collections::{BTreeMap, BTreeSet};

use zk_clob_core::{
    AccountId, BatchInput, Deposit, ExchangeConfig, ForcedWithdrawal, MAX_DEPOSITS_PER_BATCH,
    MAX_FORCED_WITHDRAWALS_PER_BATCH, MAX_ORDERS_PER_BATCH, MAX_TOUCHED_ACCOUNTS_PER_BATCH, MAX_WITHDRAWALS_PER_BATCH,
    MarketId, MarketOrderBook, SequencedOrder, Side, SignedWithdrawal, SigningDomain,
};

use crate::{AccountTree, BatchBuildError};

pub struct BatchBuilder<'a> {
    state: &'a AccountTree,
    config: &'a ExchangeConfig,
    domain: SigningDomain,
    batch_id: u64,
    old_deposit_cursor: u64,
    deposits: Vec<Deposit>,
    old_forced_withdrawal_cursor: u64,
    forced_withdrawals: Vec<ForcedWithdrawal>,
    orders: Vec<SequencedOrder>,
    withdrawals: Vec<SignedWithdrawal>,
    touched_accounts: BTreeSet<AccountId>,
    sequences: BTreeSet<u64>,
    nonces: BTreeMap<AccountId, BTreeSet<u64>>,
    books: BTreeMap<MarketId, (Vec<u32>, Vec<u32>)>,
}

impl<'a> BatchBuilder<'a> {
    pub fn new(
        state: &'a AccountTree,
        config: &'a ExchangeConfig,
        domain: SigningDomain,
        batch_id: u64,
        old_deposit_cursor: u64,
        old_forced_withdrawal_cursor: u64,
    ) -> Self {
        let mut touched_accounts = BTreeSet::new();
        touched_accounts.insert(*config.fees().recipient());
        Self {
            state,
            config,
            domain,
            batch_id,
            old_deposit_cursor,
            deposits: Vec::new(),
            old_forced_withdrawal_cursor,
            forced_withdrawals: Vec::new(),
            orders: Vec::new(),
            withdrawals: Vec::new(),
            touched_accounts,
            sequences: BTreeSet::new(),
            nonces: BTreeMap::new(),
            books: BTreeMap::new(),
        }
    }

    pub fn deposit(&mut self, deposit: Deposit) -> Result<(), BatchBuildError> {
        if self.deposits.len() >= MAX_DEPOSITS_PER_BATCH {
            return Err(BatchBuildError::TooManyDeposits);
        }
        let expected = self
            .old_deposit_cursor
            .checked_add(self.deposits.len() as u64)
            .ok_or(BatchBuildError::DepositCursorOverflow)?;
        if deposit.id() != expected {
            return Err(BatchBuildError::InvalidDepositCursor {
                expected,
                actual: deposit.id(),
            });
        }
        if deposit.amount() == 0 {
            return Err(BatchBuildError::ZeroDepositAmount);
        }
        if self.config.asset(deposit.asset()).is_none() {
            return Err(BatchBuildError::UnknownAsset(*deposit.asset()));
        }
        if !self.touched_accounts.contains(deposit.account())
            && self.touched_accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH
        {
            return Err(BatchBuildError::TooManyAccounts);
        }
        self.touched_accounts.insert(*deposit.account());
        self.deposits.push(deposit);
        Ok(())
    }

    pub fn forced_withdraw(&mut self, request: ForcedWithdrawal) -> Result<(), BatchBuildError> {
        if self.forced_withdrawals.len() >= MAX_FORCED_WITHDRAWALS_PER_BATCH {
            return Err(BatchBuildError::TooManyForcedWithdrawals);
        }
        let expected = self
            .old_forced_withdrawal_cursor
            .checked_add(self.forced_withdrawals.len() as u64)
            .ok_or(BatchBuildError::ForcedWithdrawalCursorOverflow)?;
        if request.id() != expected {
            return Err(BatchBuildError::InvalidForcedWithdrawalCursor {
                expected,
                actual: request.id(),
            });
        }
        if request.amount() == 0 {
            return Err(BatchBuildError::ZeroForcedWithdrawalAmount);
        }
        if !self.touched_accounts.contains(request.account())
            && self.touched_accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH
        {
            return Err(BatchBuildError::TooManyAccounts);
        }
        self.touched_accounts.insert(*request.account());
        self.forced_withdrawals.push(request);
        Ok(())
    }

    pub fn order(&mut self, order: SequencedOrder) -> Result<(), BatchBuildError> {
        if self.orders.len() >= MAX_ORDERS_PER_BATCH {
            return Err(BatchBuildError::TooManyOrders);
        }
        if order.price() == 0 {
            return Err(BatchBuildError::ZeroPrice);
        }
        if order.quantity() == 0 {
            return Err(BatchBuildError::ZeroQuantity);
        }
        if self.state.account(order.trader()).is_none()
            && !self.deposits.iter().any(|deposit| deposit.account() == order.trader())
        {
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
        let next_nonce = self
            .state
            .account(order.trader())
            .map_or(0, |account| account.next_nonce());
        if order.nonce() < next_nonce {
            return Err(BatchBuildError::InvalidNonce(*order.trader()));
        }
        if self
            .nonces
            .get(order.trader())
            .is_some_and(|nonces| nonces.contains(&order.nonce()))
        {
            return Err(BatchBuildError::DuplicateNonce(*order.trader(), order.nonce()));
        }
        if !self.touched_accounts.contains(order.trader())
            && self.touched_accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH
        {
            return Err(BatchBuildError::TooManyAccounts);
        }

        let index = u32::try_from(self.orders.len()).map_err(|_| BatchBuildError::OrderIndexOverflow)?;
        self.sequences.insert(order.sequence());
        self.nonces.entry(*order.trader()).or_default().insert(order.nonce());
        self.touched_accounts.insert(*order.trader());
        let (buys, sells) = self.books.entry(*order.market_id()).or_default();
        match order.side() {
            Side::Buy => buys.push(index),
            Side::Sell => sells.push(index),
        }
        self.orders.push(order);
        Ok(())
    }

    pub fn withdraw(&mut self, withdrawal: SignedWithdrawal) -> Result<(), BatchBuildError> {
        if self.withdrawals.len() >= MAX_WITHDRAWALS_PER_BATCH {
            return Err(BatchBuildError::TooManyWithdrawals);
        }
        if withdrawal.amount() == 0 {
            return Err(BatchBuildError::ZeroWithdrawalAmount);
        }
        if self.config.asset(withdrawal.asset()).is_none() {
            return Err(BatchBuildError::UnknownAsset(*withdrawal.asset()));
        }
        let account = self
            .state
            .account(withdrawal.account())
            .ok_or(BatchBuildError::UnknownAccount(*withdrawal.account()))?;
        let already_withdrawing = self
            .withdrawals
            .iter()
            .filter(|existing| existing.account() == withdrawal.account() && existing.asset() == withdrawal.asset())
            .try_fold(0u128, |total, existing| {
                total
                    .checked_add(existing.amount())
                    .ok_or(BatchBuildError::ArithmeticOverflow)
            })?;
        let required = already_withdrawing
            .checked_add(withdrawal.amount())
            .ok_or(BatchBuildError::ArithmeticOverflow)?;
        let available = account.balance(withdrawal.asset());
        if available < required {
            return Err(BatchBuildError::InsufficientBalance {
                account: *withdrawal.account(),
                asset: *withdrawal.asset(),
                available,
                required,
            });
        }
        if withdrawal.nonce() < account.next_nonce() {
            return Err(BatchBuildError::InvalidNonce(*withdrawal.account()));
        }
        if self
            .nonces
            .get(withdrawal.account())
            .is_some_and(|nonces| nonces.contains(&withdrawal.nonce()))
        {
            return Err(BatchBuildError::DuplicateNonce(
                *withdrawal.account(),
                withdrawal.nonce(),
            ));
        }
        if !self.touched_accounts.contains(withdrawal.account())
            && self.touched_accounts.len() >= MAX_TOUCHED_ACCOUNTS_PER_BATCH
        {
            return Err(BatchBuildError::TooManyAccounts);
        }

        self.nonces
            .entry(*withdrawal.account())
            .or_default()
            .insert(withdrawal.nonce());
        self.touched_accounts.insert(*withdrawal.account());
        self.withdrawals.push(withdrawal);
        Ok(())
    }

    pub fn build(self) -> Result<BatchInput, BatchBuildError> {
        for (account_id, nonces) in &self.nonces {
            let mut expected = self.state.account(account_id).map_or(0, |account| account.next_nonce());
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
                    Side::Buy.compare_priority(&self.orders[*left as usize], &self.orders[*right as usize])
                });
                sells.sort_unstable_by(|left, right| {
                    Side::Sell.compare_priority(&self.orders[*left as usize], &self.orders[*right as usize])
                });
                MarketOrderBook::new(market_id, buys, sells)
            })
            .collect();

        Ok(BatchInput::new(
            self.domain.protocolVersion,
            self.domain.chainId,
            self.domain.exchangeId,
            self.batch_id,
            self.state.root(),
            state_witness,
            self.old_deposit_cursor,
            self.deposits,
            self.old_forced_withdrawal_cursor,
            self.forced_withdrawals,
            self.orders,
            self.withdrawals,
            order_books,
            self.config.clone(),
        ))
    }
}
