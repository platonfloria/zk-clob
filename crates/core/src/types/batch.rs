use sha2::{Digest, Sha256};

use super::{
    Account, AccountId, BatchHash, ConfigHash, ExchangeConfig, ExchangeId, MarketId, Order,
    StateRoot, TradesHash,
};
use crate::hashing::Sha256Hash;

/// Private witness and transition context supplied to the SP1 guest.
pub struct BatchInput {
    pub(crate) metadata: BatchMetadata,
    pub(crate) expected_old_state_root: StateRoot,
    pub(crate) accounts: Vec<Account>,
    pub(crate) orders: Vec<Order>,
    pub(crate) config: ExchangeConfig,
}

pub(crate) struct BatchMetadata {
    protocol_version: u32,
    chain_id: u64,
    exchange_id: ExchangeId,
    batch_id: u64,
}

impl Sha256Hash for BatchMetadata {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.protocol_version.to_be_bytes());
        hasher.update(self.chain_id.to_be_bytes());
        self.exchange_id.update_hash(hasher);
        hasher.update(self.batch_id.to_be_bytes());
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
        orders: Vec<Order>,
        config: ExchangeConfig,
    ) -> Self {
        Self {
            metadata: BatchMetadata {
                protocol_version,
                chain_id,
                exchange_id,
                batch_id,
            },
            expected_old_state_root,
            accounts,
            orders,
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

    pub(crate) const fn market_id(&self) -> &MarketId {
        &self.market_id
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

pub struct PublicOutput {
    protocol_version: u32,
    chain_id: u64,
    exchange_id: ExchangeId,
    batch_id: u64,
    old_state_root: StateRoot,
    new_state_root: StateRoot,
    /// Binds the proof to the approved market and fee configuration.
    config_hash: ConfigHash,
    batch_hash: BatchHash,
    trades_hash: TradesHash,
}

impl PublicOutput {
    pub(crate) fn new(
        metadata: &BatchMetadata,
        old_state_root: StateRoot,
        new_state_root: StateRoot,
        config_hash: ConfigHash,
        batch_hash: BatchHash,
        trades_hash: TradesHash,
    ) -> Self {
        Self {
            protocol_version: metadata.protocol_version,
            chain_id: metadata.chain_id,
            exchange_id: metadata.exchange_id,
            batch_id: metadata.batch_id,
            old_state_root,
            new_state_root,
            config_hash,
            batch_hash,
            trades_hash,
        }
    }
}
