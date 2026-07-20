use sha2::{Digest as _, Sha256};

use super::{AccountId, MarketId};
use crate::hashing::Sha256Hash;

pub struct Trade {
    market_id: MarketId,
    buyer: AccountId,
    seller: AccountId,
    price: u128,
    quantity: u128,
    quote_amount: u128,
    quote_fee: u128,
}

impl Trade {
    pub(crate) const fn new(
        market_id: MarketId,
        buyer: AccountId,
        seller: AccountId,
        price: u128,
        quantity: u128,
        quote_amount: u128,
        quote_fee: u128,
    ) -> Self {
        Self {
            market_id,
            buyer,
            seller,
            price,
            quantity,
            quote_amount,
            quote_fee,
        }
    }

    pub const fn quantity(&self) -> u128 {
        self.quantity
    }

    pub const fn market_id(&self) -> &MarketId {
        &self.market_id
    }

    pub const fn buyer(&self) -> &AccountId {
        &self.buyer
    }

    pub const fn seller(&self) -> &AccountId {
        &self.seller
    }

    pub const fn price(&self) -> u128 {
        self.price
    }

    pub const fn quote_amount(&self) -> u128 {
        self.quote_amount
    }

    pub const fn quote_fee(&self) -> u128 {
        self.quote_fee
    }
}

impl Sha256Hash for Trade {
    fn update_hash(&self, hasher: &mut Sha256) {
        self.market_id.update_hash(hasher);
        self.buyer.update_hash(hasher);
        self.seller.update_hash(hasher);
        hasher.update(self.price.to_be_bytes());
        hasher.update(self.quantity.to_be_bytes());
        hasher.update(self.quote_amount.to_be_bytes());
        hasher.update(self.quote_fee.to_be_bytes());
    }
}
