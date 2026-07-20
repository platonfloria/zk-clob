use alloy_primitives::{Address, B256};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::{
    hashing::Sha256Hash,
    trees::{patricia::PatriciaKey, smt::SparseMerkleKey},
};

/// Protocol-level identifier for an asset.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AssetId(B256);

impl AssetId {
    pub const fn new(value: B256) -> Self {
        Self(value)
    }
}

impl Sha256Hash for AssetId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AccountId(Address);

impl AccountId {
    pub const fn new(value: Address) -> Self {
        Self(value)
    }
}

impl SparseMerkleKey for AccountId {
    const BITS: usize = 160;

    fn bit(&self, index: usize) -> bool {
        self.0.as_slice()[index / 8] & (1 << (7 - index % 8)) != 0
    }
}

impl PatriciaKey for AccountId {
    const BITS: usize = 160;

    fn bit(&self, index: usize) -> bool {
        self.0.as_slice()[index / 8] & (1 << (7 - index % 8)) != 0
    }
}

impl Sha256Hash for AccountId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}

/// Identifier derived from, or uniquely bound to, a market configuration.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MarketId(B256);

impl MarketId {
    pub const fn new(value: B256) -> Self {
        Self(value)
    }
}

impl Sha256Hash for MarketId {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.0);
    }
}
