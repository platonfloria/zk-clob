use std::collections::BTreeMap;

use crate::{
    Account, AccountId, AssetId, BatchHash, BatchInput, BatchMetadata, BatchOutput, ConfigHash, Deposit,
    ExecutedWithdrawal, MAX_TOUCHED_ACCOUNTS_PER_BATCH, PublicOutput, SequencedOrder, SettlementError,
    SignedWithdrawal, StateRoot, Trade,
    hashing::DomainSha256Hash as _,
    matching::match_and_settle,
    validation::{
        build_validated_books, validate_accounts, validate_config, validate_deposits, validate_limits, validate_nonces,
        validate_orders, validate_withdrawals,
    },
};

#[derive(Default)]
struct AssetTracker {
    totals: BTreeMap<AssetId, u128>,
}

impl AssetTracker {
    fn add(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        let total = self.totals.entry(asset).or_default();
        *total = total.checked_add(amount).ok_or(SettlementError::ArithmeticOverflow)?;
        Ok(())
    }

    fn subtract(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        let total = self
            .totals
            .get_mut(&asset)
            .ok_or(SettlementError::AssetConservationViolation)?;
        *total = total
            .checked_sub(amount)
            .ok_or(SettlementError::AssetConservationViolation)?;
        if *total == 0 {
            self.totals.remove(&asset);
        }
        Ok(())
    }

    fn add_accounts(&mut self, accounts: &[Account]) -> Result<(), SettlementError> {
        for account in accounts {
            for balance in account.balances() {
                self.add(*balance.asset(), balance.available())?;
            }
        }
        Ok(())
    }

    fn add_deposits(&mut self, deposits: &[Deposit]) -> Result<(), SettlementError> {
        for deposit in deposits {
            self.add(*deposit.asset(), deposit.amount())?;
        }
        Ok(())
    }

    fn subtract_withdrawals(&mut self, withdrawals: &[SignedWithdrawal]) -> Result<(), SettlementError> {
        for withdrawal in withdrawals {
            self.subtract(*withdrawal.asset(), withdrawal.amount())?;
        }
        Ok(())
    }

    fn subtract_accounts(&mut self, accounts: &[Account]) -> Result<(), SettlementError> {
        for account in accounts {
            for balance in account.balances() {
                self.subtract(*balance.asset(), balance.available())?;
            }
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.totals.is_empty()
    }
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
    withdrawals: &[SignedWithdrawal],
) -> BatchOutput {
    let executed_withdrawals: Vec<_> = withdrawals.iter().map(ExecutedWithdrawal::from).collect();
    cycle_tracker!["output-construction", {
        BatchOutput::new(
            PublicOutput::new(
                metadata,
                old_state_root,
                new_state_root,
                config_hash,
                batch_hash,
                trades.hash(),
                old_deposit_cursor,
                new_deposit_cursor,
                deposits.hash(),
                executed_withdrawals.as_slice().hash(),
            ),
            accounts,
            trades,
            executed_withdrawals,
        )
    }]
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn apply_deposits(accounts: &mut Vec<Account>, deposits: &[Deposit]) -> Result<(), SettlementError> {
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
    withdrawals: &[SignedWithdrawal],
) -> Result<(), SettlementError> {
    let mut operation_counts: BTreeMap<AccountId, u64> = BTreeMap::new();
    for order in orders {
        let count = operation_counts.entry(*order.trader()).or_default();
        *count = count.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
    }
    for withdrawal in withdrawals {
        let count = operation_counts.entry(*withdrawal.account()).or_default();
        *count = count.checked_add(1).ok_or(SettlementError::NonceOverflow)?;
    }

    let mut next_nonces = BTreeMap::new();
    for account in accounts.iter() {
        let operation_count = operation_counts.get(account.id()).copied().unwrap_or(0);
        let next_nonce = account
            .next_nonce()
            .checked_add(operation_count)
            .ok_or(SettlementError::NonceOverflow)?;
        next_nonces.insert(*account.id(), next_nonce);
    }

    for account in accounts {
        account.set_next_nonce(next_nonces[account.id()]);
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
fn apply_withdrawals(accounts: &mut [Account], withdrawals: &[SignedWithdrawal]) -> Result<(), SettlementError> {
    for withdrawal in withdrawals {
        let account = accounts
            .binary_search_by(|account| account.id().cmp(withdrawal.account()))
            .map_err(|_| SettlementError::UnknownAccount)?;
        accounts[account].debit(*withdrawal.asset(), withdrawal.amount())?;
    }
    Ok(())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    cycle_tracker![
        "batch-validation",
        validate_limits(&input)?;
        let (
            metadata,
            expected_old_state_root,
            mut state,
            old_deposit_cursor,
            deposits,
            orders,
            withdrawals,
            order_books,
            config,
        ) = (
            input.metadata,
            input.expected_old_state_root,
            input.state,
            input.old_deposit_cursor,
            input.deposits,
            input.orders,
            input.withdrawals,
            input.order_books,
            input.config,
        );

        validate_config(&config, state.accounts())?;
        validate_accounts(state.accounts())?;
        let new_deposit_cursor = validate_deposits(&deposits, old_deposit_cursor, &config)?;
    ];

    cycle_tracker![
        "input-commitments",
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
        "conservation-baseline",
        let mut asset_tracker = AssetTracker::default();
        asset_tracker.add_accounts(state.accounts())?;
        asset_tracker.add_deposits(&deposits)?;
        asset_tracker.subtract_withdrawals(&withdrawals)?;
    ];

    apply_deposits(state.accounts_mut(), &deposits)?;

    cycle_tracker![
        "operation-validation",
        validate_withdrawals(&withdrawals, state.accounts(), &config)?;
        validate_orders(&orders, state.accounts(), &config)?;
        validate_nonces(&orders, &withdrawals, state.accounts())?;
    ];

    apply_withdrawals(state.accounts_mut(), &withdrawals)?;
    consume_nonces(state.accounts_mut(), &orders, &withdrawals)?;
    let books = build_validated_books(&orders, &order_books, &config)?;
    let trades = match_and_settle(state.accounts_mut(), books, &config)?;

    cycle_tracker![
        "conservation-check",
        asset_tracker.subtract_accounts(state.accounts())?;
        if !asset_tracker.is_empty() {
            return Err(SettlementError::AssetConservationViolation);
        }
    ];

    cycle_tracker![
        "new-state-commitment",
        let new_state_root = state.root()?;
    ];

    Ok(build_output(
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
        &withdrawals,
    ))
}
