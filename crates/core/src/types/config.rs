use super::{AccountId, AssetId, MarketId};

pub struct AssetConfig {
    id: AssetId,
    /// Number of smallest units in one whole asset.
    scale: u128,
}

impl AssetConfig {
    pub const fn new(id: AssetId, scale: u128) -> Self {
        Self { id, scale }
    }
}

/// Canonical configuration of one supported market.
pub struct MarketConfig {
    id: MarketId,
    base_asset: AssetId,
    quote_asset: AssetId,
}

impl MarketConfig {
    pub const fn new(id: MarketId, base_asset: AssetId, quote_asset: AssetId) -> Self {
        Self {
            id,
            base_asset,
            quote_asset,
        }
    }
}

pub struct FeeConfig {
    recipient: AccountId,
    /// Fee charged to the buyer in the market's quote asset.
    buyer_fee_bps: u16,
}

impl FeeConfig {
    pub const fn new(recipient: AccountId, buyer_fee_bps: u16) -> Self {
        Self {
            recipient,
            buyer_fee_bps,
        }
    }
}

pub struct ExchangeConfig {
    /// Canonically sorted by asset ID, without duplicates.
    assets: Vec<AssetConfig>,
    /// Canonically sorted by market ID, without duplicates.
    markets: Vec<MarketConfig>,
    fees: FeeConfig,
}

impl ExchangeConfig {
    pub fn new(assets: Vec<AssetConfig>, markets: Vec<MarketConfig>, fees: FeeConfig) -> Self {
        Self {
            assets,
            markets,
            fees,
        }
    }
}
