use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AccountId, AssetId};
use crate::hashing::Sha256Hash;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Deposit {
    id: u64,
    account: AccountId,
    asset: AssetId,
    amount: u128,
}

impl Deposit {
    pub const fn new(id: u64, account: AccountId, asset: AssetId, amount: u128) -> Self {
        Self {
            id,
            account,
            asset,
            amount,
        }
    }

    pub const fn id(&self) -> u64 {
        self.id
    }

    pub const fn account(&self) -> &AccountId {
        &self.account
    }

    pub const fn asset(&self) -> &AssetId {
        &self.asset
    }

    pub const fn amount(&self) -> u128 {
        self.amount
    }
}

impl Sha256Hash for Deposit {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.id.to_be_bytes());
        self.account.update_hash(hasher);
        self.asset.update_hash(hasher);
        hasher.update(self.amount.to_be_bytes());
    }
}
