/// Protocol-level identifier for an asset.
#[derive(Clone, Copy)]
pub struct AssetId([u8; 32]);

impl AssetId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub struct AccountId([u8; 20]);

impl AccountId {
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

/// Identifier derived from, or uniquely bound to, a market configuration.
pub struct MarketId([u8; 32]);

impl MarketId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub struct ExchangeId([u8; 32]);

impl ExchangeId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub struct StateRoot([u8; 32]);

impl StateRoot {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub struct ConfigHash([u8; 32]);

pub struct BatchHash([u8; 32]);

pub struct TradesHash([u8; 32]);
