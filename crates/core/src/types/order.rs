use super::{AccountId, MarketId};

pub enum Side {
    Buy,
    Sell,
}

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
}
