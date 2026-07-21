use std::cmp::Ordering;

use alloy_primitives::{Address, keccak256};
use secp256k1::{
    Message, Secp256k1,
    ecdsa::{RecoverableSignature, RecoveryId},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{AccountId, MarketId};
use crate::hashing::{DomainSha256Hash, Sha256Hash};

/// Canonical recoverable secp256k1 signature encoded as `r || s || recovery_id`.
/// The recovery ID is encoded as either `0` or `1`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderSignature {
    r: [u8; 32],
    s: [u8; 32],
    recovery_id: u8,
}

impl OrderSignature {
    pub const fn new(r: [u8; 32], s: [u8; 32], recovery_id: u8) -> Self {
        Self { r, s, recovery_id }
    }

    pub const fn r(&self) -> &[u8; 32] {
        &self.r
    }

    pub const fn s(&self) -> &[u8; 32] {
        &self.s
    }

    pub const fn recovery_id(&self) -> u8 {
        self.recovery_id
    }

    fn recover(&self, digest: [u8; 32]) -> Option<AccountId> {
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

impl Sha256Hash for OrderSignature {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update(self.r);
        hasher.update(self.s);
        hasher.update([self.recovery_id]);
    }
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn compare_priority(self, left: &SequencedOrder, right: &SequencedOrder) -> Ordering {
        match self {
            Self::Buy => right.price().cmp(&left.price()),
            Self::Sell => left.price().cmp(&right.price()),
        }
        .then_with(|| left.sequence().cmp(&right.sequence()))
    }
}

#[derive(Deserialize, Serialize)]
pub struct Order {
    market_id: MarketId,
    side: Side,
    /// Quote smallest units paid per one whole base asset.
    price: u128,
    /// Quantity in the selected market's base smallest units.
    quantity: u128,
    nonce: u64,
}

impl Order {
    pub const fn new(
        market_id: MarketId,
        side: Side,
        price: u128,
        quantity: u128,
        nonce: u64,
    ) -> Self {
        Self {
            market_id,
            side,
            price,
            quantity,
            nonce,
        }
    }

    pub const fn market_id(&self) -> &MarketId {
        &self.market_id
    }

    pub const fn side(&self) -> Side {
        self.side
    }

    pub const fn price(&self) -> u128 {
        self.price
    }

    pub const fn quantity(&self) -> u128 {
        self.quantity
    }

    pub const fn nonce(&self) -> u64 {
        self.nonce
    }
}

impl Sha256Hash for Order {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.market_id.update_hash(hasher);
        hasher.update([match self.side {
            Side::Buy => 0,
            Side::Sell => 1,
        }]);
        hasher.update(self.price.to_be_bytes());
        hasher.update(self.quantity.to_be_bytes());
        hasher.update(self.nonce.to_be_bytes());
    }
}

impl DomainSha256Hash for Order {
    const DOMAIN: &'static [u8] = b"ZKCLOB_ORDER_V1";
}

#[derive(Deserialize, Serialize)]
pub struct SignedOrder {
    order: Order,
    trader: AccountId,
    signature: OrderSignature,
}

impl SignedOrder {
    pub const fn new(order: Order, trader: AccountId, signature: OrderSignature) -> Self {
        Self {
            order,
            trader,
            signature,
        }
    }

    pub const fn with_sequence(self, sequence: u64) -> SequencedOrder {
        SequencedOrder::new(self, sequence)
    }

    pub const fn order(&self) -> &Order {
        &self.order
    }

    pub const fn trader(&self) -> &AccountId {
        &self.trader
    }

    pub const fn signature(&self) -> &OrderSignature {
        &self.signature
    }

    pub fn has_valid_signature(&self) -> bool {
        self.signature
            .recover(self.order.hash().into())
            .is_some_and(|signer| signer == self.trader)
    }
}

#[derive(Deserialize, Serialize)]
pub struct SequencedOrder {
    signed_order: SignedOrder,
    sequence: u64,
}

impl SequencedOrder {
    pub const fn new(signed_order: SignedOrder, sequence: u64) -> Self {
        Self {
            signed_order,
            sequence,
        }
    }

    pub const fn signed_order(&self) -> &SignedOrder {
        &self.signed_order
    }

    pub const fn order(&self) -> &Order {
        self.signed_order.order()
    }

    pub const fn trader(&self) -> &AccountId {
        self.signed_order.trader()
    }

    pub const fn market_id(&self) -> &MarketId {
        self.order().market_id()
    }

    pub const fn side(&self) -> Side {
        self.order().side()
    }

    pub const fn price(&self) -> u128 {
        self.order().price()
    }

    pub const fn quantity(&self) -> u128 {
        self.order().quantity()
    }

    pub const fn nonce(&self) -> u64 {
        self.order().nonce()
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub const fn signature(&self) -> &OrderSignature {
        self.signed_order.signature()
    }

    pub fn has_valid_signature(&self) -> bool {
        self.signed_order.has_valid_signature()
    }
}

impl Sha256Hash for SequencedOrder {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.order().update_hash(hasher);
        self.trader().update_hash(hasher);
        hasher.update(self.sequence.to_be_bytes());
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use secp256k1::{PublicKey, SecretKey};

    use super::*;

    fn account_id(public_key: &PublicKey) -> AccountId {
        let uncompressed = public_key.serialize_uncompressed();
        let hash = keccak256(&uncompressed[1..]);
        AccountId::new(Address::from_slice(&hash[12..]))
    }

    fn signed_order() -> SequencedOrder {
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_byte_array(&[7; 32]).unwrap();
        let trader = account_id(&PublicKey::from_secret_key(&secp, &secret_key));
        let order = Order::new(
            MarketId::new(B256::new([3; 32])),
            Side::Buy,
            3_500_000_000,
            1_000_000_000_000_000_000,
            4,
        );
        let signature =
            secp.sign_ecdsa_recoverable(&Message::from_digest(order.hash().into()), &secret_key);
        let (recovery_id, compact) = signature.serialize_compact();
        SignedOrder::new(
            order,
            trader,
            OrderSignature::new(
                compact[..32].try_into().unwrap(),
                compact[32..].try_into().unwrap(),
                i32::from(recovery_id).try_into().unwrap(),
            ),
        )
        .with_sequence(12)
    }

    #[test]
    fn verifies_the_traders_signature() {
        assert!(signed_order().has_valid_signature());
    }

    #[test]
    fn signature_does_not_authorize_changed_order_terms() {
        let mut order = signed_order();
        order.signed_order.order.quantity += 1;

        assert!(!order.has_valid_signature());
    }

    #[test]
    fn operator_assigned_sequence_is_not_signed() {
        let mut order = signed_order();
        order.sequence += 1;

        assert!(order.has_valid_signature());
    }
}
