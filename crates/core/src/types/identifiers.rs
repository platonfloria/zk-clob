use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::hashing::Sha256Hash;

/// Protocol-level identifier for an asset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AssetId([u8; 32]);

impl AssetId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for AssetId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AccountId([u8; 20]);

impl AccountId {
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for AccountId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

/// Identifier derived from, or uniquely bound to, a market configuration.
#[derive(Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MarketId([u8; 32]);

impl MarketId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for MarketId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct ExchangeId([u8; 32]);

impl ExchangeId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for ExchangeId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StateRoot([u8; 32]);

impl StateRoot {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for StateRoot {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ConfigHash([u8; 32]);

impl ConfigHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for ConfigHash {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BatchHash([u8; 32]);

impl BatchHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for BatchHash {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TradesHash([u8; 32]);

impl TradesHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256Hash for TradesHash {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}
