use super::{
    Account, AccountId, BatchHash, ConfigHash, ExchangeConfig, ExchangeId, MarketId, Order,
    StateRoot, TradesHash,
};

/// Private witness and transition context supplied to the SP1 guest.
pub struct BatchInput {
    protocol_version: u32,
    chain_id: u64,
    exchange_id: ExchangeId,
    batch_id: u64,
    expected_old_state_root: StateRoot,
    accounts: Vec<Account>,
    orders: Vec<Order>,
    config: ExchangeConfig,
}

impl BatchInput {
    #[allow(clippy::too_many_arguments)]
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
            protocol_version,
            chain_id,
            exchange_id,
            batch_id,
            expected_old_state_root,
            accounts,
            orders,
            config,
        }
    }

    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    pub const fn expected_old_state_root(&self) -> &StateRoot {
        &self.expected_old_state_root
    }
}

pub struct BatchOutput {
    public: PublicOutput,
    updated_accounts: Vec<Account>,
    trades: Vec<Trade>,
}

impl BatchOutput {
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
    pub const fn quantity(&self) -> u128 {
        self.quantity
    }

    pub const fn quote_amount(&self) -> u128 {
        self.quote_amount
    }

    pub const fn quote_fee(&self) -> u128 {
        self.quote_fee
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
    /// Canonically sorted by market ID, without duplicates or zero-volume entries.
    markets: Vec<MarketSummary>,
}

/// Public accounting for one market touched by a batch.
pub struct MarketSummary {
    market_id: MarketId,
    base_volume: u128,
    quote_volume: u128,
    /// Buyer fees collected in this market's quote asset.
    quote_fees: u128,
}
