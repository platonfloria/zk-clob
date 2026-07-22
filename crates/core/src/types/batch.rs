use alloy_primitives::{Address, B256};
use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{
    Account, Deposit, ExchangeConfig, ExecutedWithdrawal, ForcedWithdrawal, MarketId, SequencedOrder, SignedWithdrawal,
    Trade,
};
use crate::{
    StateWitness,
    hashing::{DomainSha256Hash, Sha256Hash},
};

pub type ExchangeId = Address;
pub type StateRoot = B256;
pub type ConfigHash = B256;
pub type BatchHash = B256;
pub type TradesHash = B256;
pub type ConsumedDepositsHash = B256;
pub type ConsumedForcedWithdrawalsHash = B256;
pub type WithdrawalsHash = B256;
pub type ForcedWithdrawalsHash = B256;
pub type SigningDomainHash = B256;

sol! {
    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct SigningDomain {
        uint32 protocolVersion;
        uint64 chainId;
        address exchangeId;
    }

    #[derive(Debug, Eq, PartialEq)]
    struct PublicOutput {
        SigningDomain domain;
        uint64 batchId;
        bytes32 oldStateRoot;
        bytes32 newStateRoot;
        bytes32 configHash;
        bytes32 batchHash;
        bytes32 tradesHash;
        uint64 oldDepositCursor;
        uint64 newDepositCursor;
        bytes32 consumedDepositsHash;
        uint64 oldForcedWithdrawalCursor;
        bytes32 consumedForcedWithdrawalsHash;
        bytes32 withdrawalsHash;
        bytes32 forcedWithdrawalsHash;
    }
}

/// Private witness and transition context supplied to the SP1 guest.
#[derive(Deserialize, Serialize)]
pub struct BatchInput {
    pub(crate) domain: SigningDomain,
    pub(crate) batch_id: u64,
    pub expected_old_state_root: StateRoot,
    pub(crate) state: StateWitness,
    pub(crate) old_deposit_cursor: u64,
    pub(crate) deposits: Vec<Deposit>,
    pub(crate) old_forced_withdrawal_cursor: u64,
    pub(crate) forced_withdrawals: Vec<ForcedWithdrawal>,
    pub(crate) orders: Vec<SequencedOrder>,
    pub(crate) withdrawals: Vec<SignedWithdrawal>,
    pub(crate) order_books: Vec<MarketOrderBook>,
    pub(crate) config: ExchangeConfig,
}

/// Canonical host-built order-book view for one market.
#[derive(Deserialize, Serialize)]
pub struct MarketOrderBook {
    market_id: MarketId,
    buy_indices: Vec<u32>,
    sell_indices: Vec<u32>,
}

impl MarketOrderBook {
    pub const fn new(market_id: MarketId, buy_indices: Vec<u32>, sell_indices: Vec<u32>) -> Self {
        Self {
            market_id,
            buy_indices,
            sell_indices,
        }
    }

    pub const fn market_id(&self) -> &MarketId {
        &self.market_id
    }

    pub fn buy_indices(&self) -> &[u32] {
        &self.buy_indices
    }

    pub fn sell_indices(&self) -> &[u32] {
        &self.sell_indices
    }
}

impl SigningDomain {
    pub const fn new(protocol_version: u32, chain_id: u64, exchange_id: ExchangeId) -> Self {
        Self {
            protocolVersion: protocol_version,
            chainId: chain_id,
            exchangeId: exchange_id,
        }
    }
}

impl Sha256Hash for SigningDomain {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.protocolVersion.to_be_bytes());
        hasher.update(self.chainId.to_be_bytes());
        hasher.update(self.exchangeId);
    }
}

impl DomainSha256Hash for SigningDomain {
    const DOMAIN: &'static [u8] = b"ZKCLOB_SIGNING_DOMAIN_V1";
}

impl BatchInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        protocol_version: u32,
        chain_id: u64,
        exchange_id: ExchangeId,
        batch_id: u64,
        expected_old_state_root: StateRoot,
        state: StateWitness,
        old_deposit_cursor: u64,
        deposits: Vec<Deposit>,
        old_forced_withdrawal_cursor: u64,
        forced_withdrawals: Vec<ForcedWithdrawal>,
        orders: Vec<SequencedOrder>,
        withdrawals: Vec<SignedWithdrawal>,
        order_books: Vec<MarketOrderBook>,
        config: ExchangeConfig,
    ) -> Self {
        Self {
            domain: SigningDomain::new(protocol_version, chain_id, exchange_id),
            batch_id,
            expected_old_state_root,
            state,
            old_deposit_cursor,
            deposits,
            old_forced_withdrawal_cursor,
            forced_withdrawals,
            orders,
            withdrawals,
            order_books,
            config,
        }
    }

    pub fn withdrawals(&self) -> &[SignedWithdrawal] {
        &self.withdrawals
    }

    pub fn forced_withdrawals(&self) -> &[ForcedWithdrawal] {
        &self.forced_withdrawals
    }
}

pub struct BatchOutput {
    public: PublicOutput,
    updated_accounts: Vec<Account>,
    trades: Vec<Trade>,
    withdrawals: Vec<ExecutedWithdrawal>,
    forced_withdrawals: Vec<ForcedWithdrawal>,
}

impl BatchOutput {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        public: PublicOutput,
        updated_accounts: Vec<Account>,
        trades: Vec<Trade>,
        withdrawals: Vec<ExecutedWithdrawal>,
        forced_withdrawals: Vec<ForcedWithdrawal>,
    ) -> Self {
        Self {
            public,
            updated_accounts,
            trades,
            withdrawals,
            forced_withdrawals,
        }
    }

    pub fn updated_accounts(&self) -> &[Account] {
        &self.updated_accounts
    }

    pub const fn public(&self) -> &PublicOutput {
        &self.public
    }

    pub fn trades(&self) -> &[Trade] {
        &self.trades
    }

    pub fn withdrawals(&self) -> &[ExecutedWithdrawal] {
        &self.withdrawals
    }

    pub fn forced_withdrawals(&self) -> &[ForcedWithdrawal] {
        &self.forced_withdrawals
    }
}

impl PublicOutput {
    pub(crate) fn new(
        domain: SigningDomain,
        batch_id: u64,
        old_state_root: StateRoot,
        new_state_root: StateRoot,
        config_hash: ConfigHash,
        batch_hash: BatchHash,
        trades_hash: TradesHash,
        old_deposit_cursor: u64,
        new_deposit_cursor: u64,
        consumed_deposits_hash: ConsumedDepositsHash,
        old_forced_withdrawal_cursor: u64,
        consumed_forced_withdrawals_hash: ConsumedForcedWithdrawalsHash,
        withdrawals_hash: WithdrawalsHash,
        forced_withdrawals_hash: ForcedWithdrawalsHash,
    ) -> Self {
        Self {
            domain,
            batchId: batch_id,
            oldStateRoot: old_state_root,
            newStateRoot: new_state_root,
            configHash: config_hash,
            batchHash: batch_hash,
            tradesHash: trades_hash,
            oldDepositCursor: old_deposit_cursor,
            newDepositCursor: new_deposit_cursor,
            consumedDepositsHash: consumed_deposits_hash,
            oldForcedWithdrawalCursor: old_forced_withdrawal_cursor,
            consumedForcedWithdrawalsHash: consumed_forced_withdrawals_hash,
            withdrawalsHash: withdrawals_hash,
            forcedWithdrawalsHash: forced_withdrawals_hash,
        }
    }
}
