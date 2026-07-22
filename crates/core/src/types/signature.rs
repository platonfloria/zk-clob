use alloy_primitives::{Address, keccak256};
use secp256k1::{
    Message, Secp256k1,
    ecdsa::{RecoverableSignature, RecoveryId},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AccountId, SigningDomainHash};
use crate::hashing::{DomainSha256Hash, Sha256Hash};

pub trait SignableOperation: DomainSha256Hash {
    fn digest(&self, domain_hash: &SigningDomainHash) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"ZKCLOB_SIGNED_OPERATION_V1");
        hasher.update(domain_hash);
        hasher.update(self.hash());
        hasher.finalize().into()
    }
}

impl<T> SignableOperation for T where T: DomainSha256Hash {}

#[derive(Deserialize, Serialize)]
pub struct SignedOperation<O: SignableOperation> {
    operation: O,
    signer: AccountId,
    signature: Signature,
    account_index: Option<u32>,
}

impl<O: SignableOperation> SignedOperation<O> {
    pub const fn new(operation: O, signer: AccountId, signature: Signature) -> Self {
        Self {
            operation,
            signer,
            signature,
            account_index: None,
        }
    }

    pub fn with_account_index(mut self, account_index: u32) -> Self {
        self.account_index = Some(account_index);
        self
    }

    pub const fn operation(&self) -> &O {
        &self.operation
    }

    #[cfg(test)]
    pub(crate) const fn operation_mut(&mut self) -> &mut O {
        &mut self.operation
    }

    pub const fn signer(&self) -> &AccountId {
        &self.signer
    }

    pub const fn account_index(&self) -> Option<u32> {
        self.account_index
    }

    pub const fn signature(&self) -> &Signature {
        &self.signature
    }
}

impl<O: SignableOperation> SignedOperation<O> {
    pub fn has_valid_signature(&self, domain_hash: &SigningDomainHash) -> bool {
        self.signature
            .recover(self.operation.digest(domain_hash))
            .is_some_and(|signer| signer == self.signer)
    }
}

/// Canonical recoverable secp256k1 signature encoded as `r || s || recovery_id`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Signature {
    r: [u8; 32],
    s: [u8; 32],
    recovery_id: u8,
}

impl Signature {
    pub const fn new(r: [u8; 32], s: [u8; 32], recovery_id: u8) -> Self {
        Self { r, s, recovery_id }
    }

    pub(crate) fn recover(&self, digest: [u8; 32]) -> Option<AccountId> {
        if self.recovery_id > 1 {
            return None;
        }
        let recovery_id = RecoveryId::try_from(i32::from(self.recovery_id)).ok()?;
        let mut compact = [0; 64];
        compact[..32].copy_from_slice(&self.r);
        compact[32..].copy_from_slice(&self.s);

        let signature = RecoverableSignature::from_compact(&compact, recovery_id).ok()?;
        let standard = signature.to_standard();
        let mut normalized = standard;
        normalized.normalize_s();
        if normalized != standard {
            return None;
        }

        let public_key = Secp256k1::verification_only()
            .recover_ecdsa(&Message::from_digest(digest), &signature)
            .ok()?;
        let uncompressed = public_key.serialize_uncompressed();
        let hash = keccak256(&uncompressed[1..]);
        Some(AccountId::new(Address::from_slice(&hash[12..])))
    }
}

impl Sha256Hash for Signature {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.r);
        hasher.update(self.s);
        hasher.update([self.recovery_id]);
    }
}
