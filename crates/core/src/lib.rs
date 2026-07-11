mod consts;
mod errors;
mod hashing;
mod matching;
mod settlement;
mod types;
mod validation;

pub use errors::SettlementError;
pub use hashing::{
    compute_batch_hash, compute_config_hash, compute_state_root, compute_trades_hash,
};
pub use settlement::settle_batch;
pub use types::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchHash, BatchInput, BatchMetadata,
    BatchOutput, ConfigHash, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig, MarketId, Order,
    PublicOutput, Side, StateRoot, Trade, TradesHash,
};
