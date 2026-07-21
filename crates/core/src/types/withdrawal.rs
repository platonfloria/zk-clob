use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AccountId, AssetId, SignedOperation};
use crate::hashing::{DomainSha256Hash, Sha256Hash};

/// A user's unsigned intent to remove an asset from the exchange state.
#[derive(Deserialize, Serialize)]
pub struct Withdrawal {
    asset: AssetId,
    amount: u128,
    recipient: AccountId,
    nonce: u64,
}

impl Withdrawal {
    pub const fn new(asset: AssetId, amount: u128, recipient: AccountId, nonce: u64) -> Self {
        Self {
            asset,
            amount,
            recipient,
            nonce,
        }
    }

    pub const fn asset(&self) -> &AssetId {
        &self.asset
    }

    pub const fn amount(&self) -> u128 {
        self.amount
    }

    pub const fn recipient(&self) -> &AccountId {
        &self.recipient
    }

    pub const fn nonce(&self) -> u64 {
        self.nonce
    }
}

impl Sha256Hash for Withdrawal {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.asset.update_hash(hasher);
        hasher.update(self.amount.to_be_bytes());
        self.recipient.update_hash(hasher);
        hasher.update(self.nonce.to_be_bytes());
    }
}

impl DomainSha256Hash for Withdrawal {
    const DOMAIN: &'static [u8] = b"ZKCLOB_WITHDRAWAL_V1";
}

pub type SignedWithdrawal = SignedOperation<Withdrawal>;

impl SignedWithdrawal {
    pub const fn withdrawal(&self) -> &Withdrawal {
        self.operation()
    }

    pub const fn account(&self) -> &AccountId {
        self.signer()
    }

    pub const fn asset(&self) -> &AssetId {
        self.operation().asset()
    }

    pub const fn amount(&self) -> u128 {
        self.operation().amount()
    }

    pub const fn recipient(&self) -> &AccountId {
        self.operation().recipient()
    }

    pub const fn nonce(&self) -> u64 {
        self.operation().nonce()
    }
}

/// Canonical withdrawal record committed publicly and executed by the settlement contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExecutedWithdrawal {
    account: AccountId,
    recipient: AccountId,
    asset: AssetId,
    amount: u128,
    nonce: u64,
}

impl ExecutedWithdrawal {
    pub const fn account(&self) -> &AccountId {
        &self.account
    }

    pub const fn recipient(&self) -> &AccountId {
        &self.recipient
    }

    pub const fn asset(&self) -> &AssetId {
        &self.asset
    }

    pub const fn amount(&self) -> u128 {
        self.amount
    }

    pub const fn nonce(&self) -> u64 {
        self.nonce
    }
}

impl From<&SignedWithdrawal> for ExecutedWithdrawal {
    fn from(value: &SignedWithdrawal) -> Self {
        Self {
            account: *value.account(),
            recipient: *value.recipient(),
            asset: *value.asset(),
            amount: value.amount(),
            nonce: value.nonce(),
        }
    }
}

impl Sha256Hash for ExecutedWithdrawal {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.account.update_hash(hasher);
        self.recipient.update_hash(hasher);
        self.asset.update_hash(hasher);
        hasher.update(self.amount.to_be_bytes());
        hasher.update(self.nonce.to_be_bytes());
    }
}

impl Sha256Hash for [ExecutedWithdrawal] {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update((self.len() as u64).to_be_bytes());
        for withdrawal in self {
            withdrawal.update_hash(hasher);
        }
    }
}

impl DomainSha256Hash for [ExecutedWithdrawal] {
    const DOMAIN: &'static [u8] = b"ZKCLOB_WITHDRAWALS_V1";
}
