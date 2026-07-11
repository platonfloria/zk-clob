use sha2::{Digest, Sha256};

use super::{AccountId, AssetId};
use crate::hashing::Sha256Hash;

pub struct Account {
    id: AccountId,
    /// Canonically sorted by asset ID, without duplicates or zero balances.
    balances: Vec<AssetBalance>,
    next_nonce: u64,
}

impl Account {
    pub fn new(id: AccountId, balances: Vec<AssetBalance>, next_nonce: u64) -> Self {
        Self {
            id,
            balances,
            next_nonce,
        }
    }

    pub const fn id(&self) -> &AccountId {
        &self.id
    }

    pub fn balances(&self) -> &[AssetBalance] {
        &self.balances
    }

    pub const fn next_nonce(&self) -> u64 {
        self.next_nonce
    }

    pub(crate) const fn set_next_nonce(&mut self, next_nonce: u64) {
        self.next_nonce = next_nonce;
    }

    pub fn balance(&self, asset: &AssetId) -> u128 {
        self.balances
            .iter()
            .find(|balance| balance.asset.as_bytes() == asset.as_bytes())
            .map_or(0, |balance| balance.available)
    }
}

impl Sha256Hash for Account {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.id.update_hash(hasher);
        hasher.update(self.next_nonce.to_be_bytes());
        hasher.update((self.balances.len() as u64).to_be_bytes());

        for balance in &self.balances {
            balance.update_hash(hasher);
        }
    }
}

pub struct AssetBalance {
    asset: AssetId,
    available: u128,
}

impl AssetBalance {
    pub const fn new(asset: AssetId, available: u128) -> Self {
        Self { asset, available }
    }

    pub const fn asset(&self) -> &AssetId {
        &self.asset
    }

    pub const fn available(&self) -> u128 {
        self.available
    }
}

impl Sha256Hash for AssetBalance {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.asset.update_hash(hasher);
        hasher.update(self.available.to_be_bytes());
    }
}
