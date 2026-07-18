use alloy_primitives::B256;
use alloy_sol_types::sol;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Account, AccountId, ExchangeConfig, MarketId, Order};
use crate::hashing::Sha256Hash;

pub type ExchangeId = B256;
pub type StateRoot = B256;
pub type ConfigHash = B256;
pub type BatchHash = B256;
pub type TradesHash = B256;

sol! {
    #[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
    struct BatchMetadata {
        uint32 protocolVersion;
        uint64 chainId;
        bytes32 exchangeId;
        uint64 batchId;
    }

    #[derive(Debug, Eq, PartialEq)]
    struct PublicOutput {
        BatchMetadata metadata;
        bytes32 oldStateRoot;
        bytes32 newStateRoot;
        bytes32 configHash;
        bytes32 batchHash;
        bytes32 tradesHash;
    }
}

/// Private witness and transition context supplied to the SP1 guest.
#[derive(Deserialize, Serialize)]
pub struct BatchInput {
    pub(crate) metadata: BatchMetadata,
    pub expected_old_state_root: StateRoot,
    pub(crate) accounts: Vec<Account>,
    pub(crate) state_multiproof: StateMultiproof,
    pub(crate) orders: Vec<Order>,
    pub(crate) order_books: Vec<MarketOrderBook>,
    pub(crate) config: ExchangeConfig,
}

/// Canonically ordered hashes of untouched sibling subtrees in the account DMT.
#[derive(Deserialize, Serialize)]
pub struct StateMultiproof {
    leaf_count: u32,
    leaf_indices: Vec<u32>,
    side_nodes: Vec<StateRoot>,
}

impl StateMultiproof {
    pub const fn new(leaf_count: u32, leaf_indices: Vec<u32>, side_nodes: Vec<StateRoot>) -> Self {
        Self {
            leaf_count,
            leaf_indices,
            side_nodes,
        }
    }

    pub const fn leaf_count(&self) -> u32 {
        self.leaf_count
    }

    pub fn leaf_indices(&self) -> &[u32] {
        &self.leaf_indices
    }

    pub fn side_nodes(&self) -> &[StateRoot] {
        &self.side_nodes
    }
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

impl BatchMetadata {
    pub const fn new(
        protocol_version: u32,
        chain_id: u64,
        exchange_id: ExchangeId,
        batch_id: u64,
    ) -> Self {
        Self {
            protocolVersion: protocol_version,
            chainId: chain_id,
            exchangeId: exchange_id,
            batchId: batch_id,
        }
    }
}

impl Sha256Hash for BatchMetadata {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.protocolVersion.to_be_bytes());
        hasher.update(self.chainId.to_be_bytes());
        hasher.update(self.exchangeId);
        hasher.update(self.batchId.to_be_bytes());
    }
}

impl BatchInput {
    pub fn new(
        protocol_version: u32,
        chain_id: u64,
        exchange_id: ExchangeId,
        batch_id: u64,
        expected_old_state_root: StateRoot,
        accounts: Vec<Account>,
        state_multiproof: StateMultiproof,
        orders: Vec<Order>,
        order_books: Vec<MarketOrderBook>,
        config: ExchangeConfig,
    ) -> Self {
        Self {
            metadata: BatchMetadata::new(protocol_version, chain_id, exchange_id, batch_id),
            expected_old_state_root,
            accounts,
            state_multiproof,
            orders,
            order_books,
            config,
        }
    }
}

pub struct BatchOutput {
    public: PublicOutput,
    updated_accounts: Vec<Account>,
    trades: Vec<Trade>,
}

impl BatchOutput {
    pub(crate) fn new(
        public: PublicOutput,
        updated_accounts: Vec<Account>,
        trades: Vec<Trade>,
    ) -> Self {
        Self {
            public,
            updated_accounts,
            trades,
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
}

pub struct Trade {
    market_id: MarketId,
    buyer: AccountId,
    seller: AccountId,
    price: u128,
    quantity: u128,
    quote_amount: u128,
    quote_fee: u128,
}

impl Trade {
    pub(crate) const fn new(
        market_id: MarketId,
        buyer: AccountId,
        seller: AccountId,
        price: u128,
        quantity: u128,
        quote_amount: u128,
        quote_fee: u128,
    ) -> Self {
        Self {
            market_id,
            buyer,
            seller,
            price,
            quantity,
            quote_amount,
            quote_fee,
        }
    }

    pub const fn quantity(&self) -> u128 {
        self.quantity
    }

    pub const fn market_id(&self) -> &MarketId {
        &self.market_id
    }

    pub const fn buyer(&self) -> &AccountId {
        &self.buyer
    }

    pub const fn seller(&self) -> &AccountId {
        &self.seller
    }

    pub const fn price(&self) -> u128 {
        self.price
    }

    pub const fn quote_amount(&self) -> u128 {
        self.quote_amount
    }

    pub const fn quote_fee(&self) -> u128 {
        self.quote_fee
    }
}

impl Sha256Hash for Trade {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.market_id.update_hash(hasher);
        self.buyer.update_hash(hasher);
        self.seller.update_hash(hasher);
        hasher.update(self.price.to_be_bytes());
        hasher.update(self.quantity.to_be_bytes());
        hasher.update(self.quote_amount.to_be_bytes());
        hasher.update(self.quote_fee.to_be_bytes());
    }
}

impl PublicOutput {
    pub(crate) fn new(
        metadata: BatchMetadata,
        old_state_root: StateRoot,
        new_state_root: StateRoot,
        config_hash: ConfigHash,
        batch_hash: BatchHash,
        trades_hash: TradesHash,
    ) -> Self {
        Self {
            metadata,
            oldStateRoot: old_state_root,
            newStateRoot: new_state_root,
            configHash: config_hash,
            batchHash: batch_hash,
            tradesHash: trades_hash,
        }
    }
}
