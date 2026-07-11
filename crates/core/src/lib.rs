mod errors;
mod hashing;
mod settlement;
mod types;
mod validation;

pub use errors::SettlementError;
pub use hashing::compute_state_root;
pub use settlement::settle_batch;
pub use types::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchHash, BatchInput, BatchOutput,
    ConfigHash, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig, MarketId, MarketSummary,
    Order, PublicOutput, Side, StateRoot, Trade, TradesHash,
};
