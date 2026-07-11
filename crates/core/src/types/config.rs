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

    pub const fn id(&self) -> AssetId {
        self.id
    }

    pub const fn scale(&self) -> u128 {
        self.scale
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

    pub const fn id(&self) -> &MarketId {
        &self.id
    }

    pub const fn base_asset(&self) -> &AssetId {
        &self.base_asset
    }

    pub const fn quote_asset(&self) -> &AssetId {
        &self.quote_asset
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

    pub const fn recipient(&self) -> &AccountId {
        &self.recipient
    }

    pub const fn buyer_fee_bps(&self) -> u16 {
        self.buyer_fee_bps
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

    pub fn assets(&self) -> &[AssetConfig] {
        &self.assets
    }

    pub fn markets(&self) -> &[MarketConfig] {
        &self.markets
    }

    pub const fn fees(&self) -> &FeeConfig {
        &self.fees
    }

    pub(crate) fn market(&self, id: &MarketId) -> Option<&MarketConfig> {
        self.markets.iter().find(|market| market.id() == id)
    }

    pub(crate) fn asset(&self, id: &AssetId) -> Option<&AssetConfig> {
        self.assets.iter().find(|asset| &asset.id() == id)
    }
}
