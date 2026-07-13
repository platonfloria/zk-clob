use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{AccountId, AssetId, MarketId};
use crate::hashing::Sha256Hash;

#[derive(Deserialize, Serialize)]
pub struct AssetConfig {
    id: AssetId,
    /// Number of smallest units in one whole asset.
    scale: u128,
}

impl AssetConfig {
    pub const fn new(id: AssetId, scale: u128) -> Self {
        Self { id, scale }
    }

    pub const fn id(&self) -> &AssetId {
        &self.id
    }

    pub const fn scale(&self) -> u128 {
        self.scale
    }
}

impl Sha256Hash for AssetConfig {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.id.update_hash(hasher);
        hasher.update(self.scale.to_be_bytes());
    }
}

/// Canonical configuration of one supported market.
#[derive(Deserialize, Serialize)]
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

impl Sha256Hash for MarketConfig {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.id.update_hash(hasher);
        self.base_asset.update_hash(hasher);
        self.quote_asset.update_hash(hasher);
    }
}

#[derive(Deserialize, Serialize)]
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

impl Sha256Hash for FeeConfig {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.recipient.update_hash(hasher);
        hasher.update(self.buyer_fee_bps.to_be_bytes());
    }
}

#[derive(Deserialize, Serialize)]
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
        self.markets
            .binary_search_by(|market| market.id().cmp(id))
            .ok()
            .map(|index| &self.markets[index])
    }

    pub(crate) fn asset(&self, id: &AssetId) -> Option<&AssetConfig> {
        self.assets
            .binary_search_by(|asset| asset.id().cmp(id))
            .ok()
            .map(|index| &self.assets[index])
    }
}

impl Sha256Hash for ExchangeConfig {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update((self.assets.len() as u64).to_be_bytes());
        for asset in &self.assets {
            asset.update_hash(hasher);
        }
        hasher.update((self.markets.len() as u64).to_be_bytes());
        for market in &self.markets {
            market.update_hash(hasher);
        }
        self.fees.update_hash(hasher);
    }
}
