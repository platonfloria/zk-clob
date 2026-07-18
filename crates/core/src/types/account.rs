use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{AccountId, AssetId};
use crate::{
    SettlementError,
    hashing::{DomainSha256Hash, Sha256Hash},
};

#[derive(Clone, Deserialize, Serialize)]
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
            .find(|balance| balance.asset == *asset)
            .map_or(0, |balance| balance.available)
    }

    pub(crate) fn debit(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        let index = self
            .balances
            .binary_search_by_key(&asset, |balance| balance.asset)
            .map_err(|_| SettlementError::InsufficientBalance {
                account: self.id,
                asset,
                available: 0,
                required: amount,
            })?;
        let available = self.balances[index].available;
        self.balances[index].available =
            available
                .checked_sub(amount)
                .ok_or(SettlementError::InsufficientBalance {
                    account: self.id,
                    asset,
                    available,
                    required: amount,
                })?;
        if self.balances[index].available == 0 {
            self.balances.remove(index);
        }
        Ok(())
    }

    pub(crate) fn credit(&mut self, asset: AssetId, amount: u128) -> Result<(), SettlementError> {
        if amount == 0 {
            return Ok(());
        }
        match self
            .balances
            .binary_search_by_key(&asset, |balance| balance.asset)
        {
            Ok(index) => {
                self.balances[index].available = self.balances[index]
                    .available
                    .checked_add(amount)
                    .ok_or(SettlementError::ArithmeticOverflow)?;
            }
            Err(index) => self
                .balances
                .insert(index, AssetBalance::new(asset, amount)),
        }
        Ok(())
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

impl DomainSha256Hash for Account {
    const DOMAIN: &'static [u8] = b"ZKCLOB_ACCOUNT_V1";
}

#[derive(Clone, Deserialize, Serialize)]
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
