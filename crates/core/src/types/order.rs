use super::{AccountId, MarketId};

#[derive(Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy)]
pub struct Order {
    trader: AccountId,
    market_id: MarketId,
    side: Side,
    /// Quote smallest units paid per one whole base asset.
    price: u128,
    /// Quantity in the selected market's base smallest units.
    quantity: u128,
    nonce: u64,
    sequence: u64,
}

impl Order {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        trader: AccountId,
        market_id: MarketId,
        side: Side,
        price: u128,
        quantity: u128,
        nonce: u64,
        sequence: u64,
    ) -> Self {
        Self {
            trader,
            market_id,
            side,
            price,
            quantity,
            nonce,
            sequence,
        }
    }

    pub const fn trader(&self) -> &AccountId {
        &self.trader
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

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
}
