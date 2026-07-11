use sha2::{Digest, Sha256};

use crate::hashing::Sha256Hash;

/// Protocol-level identifier for an asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AssetId([u8; 32]);

impl AssetId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Sha256Hash for AssetId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.as_bytes());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AccountId([u8; 20]);

impl AccountId {
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl Sha256Hash for AccountId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.as_bytes());
    }
}

/// Identifier derived from, or uniquely bound to, a market configuration.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(PartialEq, Eq)]
pub struct StateRoot([u8; 32]);

impl StateRoot {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub struct ConfigHash([u8; 32]);

pub struct BatchHash([u8; 32]);

pub struct TradesHash([u8; 32]);
