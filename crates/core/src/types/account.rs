use super::{AccountId, AssetId};

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

    pub fn balance(&self, asset: &AssetId) -> u128 {
        self.balances
            .iter()
            .find(|balance| balance.asset.as_bytes() == asset.as_bytes())
            .map_or(0, |balance| balance.available)
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
}
