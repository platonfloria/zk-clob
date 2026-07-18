use alloy_primitives::B256;
use sha2::{Digest as _, Sha256};

use crate::{BatchMetadata, ConfigHash, Order, StateRoot, Trade};

pub trait Sha256Hash {
    fn update_hash(&self, hasher: &mut Sha256);
}

pub trait DomainSha256Hash: Sha256Hash {
    const DOMAIN: &'static [u8];

    #[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
    fn hash(&self) -> B256 {
        let mut hasher = Sha256::new();
        hasher.update(Self::DOMAIN);
        self.update_hash(&mut hasher);
        B256::new(hasher.finalize().into())
    }
}

impl Sha256Hash for (&BatchMetadata, &StateRoot, &ConfigHash, &[Order]) {
    fn update_hash(&self, hasher: &mut Sha256) {
        let (metadata, old_state_root, config_hash, orders) = self;
        metadata.update_hash(hasher);
        hasher.update(old_state_root);
        hasher.update(config_hash);

        let mut orders: Vec<_> = orders.iter().collect();
        hasher.update((orders.len() as u64).to_be_bytes());
        orders.sort_unstable_by(|a, b| {
            a.market_id()
                .cmp(b.market_id())
                .then_with(|| a.sequence().cmp(&b.sequence()))
        });
        for order in orders {
            order.update_hash(hasher);
        }
    }
}

impl DomainSha256Hash for (&BatchMetadata, &StateRoot, &ConfigHash, &[Order]) {
    const DOMAIN: &'static [u8] = b"ZKCLOB_BATCH_V1";
}

impl Sha256Hash for Vec<Trade> {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update((self.len() as u64).to_be_bytes());
        for trade in self {
            trade.update_hash(hasher);
        }
    }
}

impl DomainSha256Hash for Vec<Trade> {
    const DOMAIN: &'static [u8] = b"ZKCLOB_TRADES_V1";
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256};

    use super::*;
    use crate::compute_state_root;
    use crate::{
        Account, AccountId, AssetBalance, AssetConfig, AssetId, ExchangeConfig, ExchangeId,
        FeeConfig, MarketConfig, MarketId, Side,
    };

    const ETH: AssetConfig = AssetConfig::new(AssetId::new(B256::new([1; 32])), 10u128.pow(18));
    const USDC: AssetConfig = AssetConfig::new(AssetId::new(B256::new([2; 32])), 10u128.pow(6));
    const MARKET: MarketId = MarketId::new(B256::new([3; 32]));
    const ALICE: AccountId = AccountId::new(Address::new([1; 20]));
    const BOB: AccountId = AccountId::new(Address::new([2; 20]));
    const TREASURY: AccountId = AccountId::new(Address::new([3; 20]));

    #[test]
    fn changing_a_balance_changes_the_state_root() {
        let account = |available| {
            vec![Account::new(
                ALICE,
                vec![AssetBalance::new(*USDC.id(), available)],
                0,
            )]
        };

        assert_ne!(
            compute_state_root(&account(100)),
            compute_state_root(&account(101))
        );
    }

    #[test]
    fn changing_fee_config_changes_the_config_hash() {
        let config = |fee| {
            ExchangeConfig::new(
                vec![ETH, USDC],
                vec![MarketConfig::new(MARKET, *ETH.id(), *USDC.id())],
                FeeConfig::new(TREASURY, fee),
            )
        };

        assert_ne!(config(10).hash(), config(11).hash());
    }

    #[test]
    fn changing_an_order_changes_the_batch_hash() {
        let metadata = BatchMetadata::new(1, 31_337, ExchangeId::new([4; 32]), 0);
        let old_state_root = StateRoot::new([7; 32]);
        let config_hash = ConfigHash::new([8; 32]);
        let order = |price| Order::new(ALICE, MARKET, Side::Buy, price, ETH.scale(), 0, 1);

        assert_ne!(
            (
                &metadata,
                &old_state_root,
                &config_hash,
                [order(100)].as_slice(),
            )
                .hash(),
            (
                &metadata,
                &old_state_root,
                &config_hash,
                [order(101)].as_slice(),
            )
                .hash()
        );
    }

    #[test]
    fn changing_the_trade_list_changes_the_trades_hash() {
        let trade = Trade::new(MARKET, ALICE, BOB, 100, 10, 1_000, 1);

        assert_ne!(vec![trade].hash(), vec![].hash());
    }
}
