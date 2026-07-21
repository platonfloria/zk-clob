use alloy_primitives::B256;
use sha2::{Digest as _, Sha256};

use crate::{BatchMetadata, ConfigHash, Deposit, SequencedOrder, StateRoot, Trade};

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

impl Sha256Hash for (&BatchMetadata, &StateRoot, &ConfigHash, &[SequencedOrder]) {
    fn update_hash(&self, hasher: &mut Sha256) {
        let (metadata, old_state_root, config_hash, orders) = self;
        metadata.update_hash(hasher);
        hasher.update(old_state_root);
        hasher.update(config_hash);

        let orders: Vec<_> = orders.iter().collect();
        hasher.update((orders.len() as u64).to_be_bytes());
        for order in orders {
            order.update_hash(hasher);
        }
    }
}

impl DomainSha256Hash for (&BatchMetadata, &StateRoot, &ConfigHash, &[SequencedOrder]) {
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

impl Sha256Hash for [Deposit] {
    fn update_hash(&self, hasher: &mut Sha256) {
        hasher.update((self.len() as u64).to_be_bytes());
        for deposit in self {
            deposit.update_hash(hasher);
        }
    }
}

impl DomainSha256Hash for [Deposit] {
    const DOMAIN: &'static [u8] = b"ZKCLOB_DEPOSITS_V1";
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use zk_clob_test_utils::{ALICE, BOB, TREASURY};

    use super::*;
    use crate::{
        Account, AccountId, AssetBalance, AssetConfig, AssetId, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig,
        MarketId, Order, OrderSignature, Side, SignedOrder, State,
    };

    const ETH: AssetConfig = AssetConfig::new(AssetId::new(B256::new([1; 32])), 10u128.pow(18));
    const USDC: AssetConfig = AssetConfig::new(AssetId::new(B256::new([2; 32])), 10u128.pow(6));
    const MARKET: MarketId = MarketId::new(B256::new([3; 32]));

    #[test]
    fn changing_a_balance_changes_the_state_root() {
        let account = |available| {
            vec![Account::new(
                AccountId::new(ALICE.address()),
                vec![AssetBalance::new(*USDC.id(), available)],
                0,
            )]
        };

        assert_ne!(State::new(account(100)).root(), State::new(account(101)).root());
    }

    #[test]
    fn changing_fee_config_changes_the_config_hash() {
        let config = |fee| {
            ExchangeConfig::new(
                vec![ETH, USDC],
                vec![MarketConfig::new(MARKET, *ETH.id(), *USDC.id())],
                FeeConfig::new(AccountId::new(TREASURY.address()), fee),
            )
        };

        assert_ne!(config(10).hash(), config(11).hash());
    }

    #[test]
    fn changing_an_order_changes_the_batch_hash() {
        let metadata = BatchMetadata::new(1, 31_337, ExchangeId::new([4; 32]), 0);
        let old_state_root = StateRoot::new([7; 32]);
        let config_hash = ConfigHash::new([8; 32]);
        let order = |price| {
            SignedOrder::new(
                Order::new(MARKET, Side::Buy, price, ETH.scale(), 0),
                AccountId::new(ALICE.address()),
                OrderSignature::new([0; 32], [0; 32], 0),
            )
            .with_sequence(1)
        };

        assert_ne!(
            (&metadata, &old_state_root, &config_hash, [order(100)].as_slice(),).hash(),
            (&metadata, &old_state_root, &config_hash, [order(101)].as_slice(),).hash()
        );
    }

    #[test]
    fn changing_the_trade_list_changes_the_trades_hash() {
        let trade = Trade::new(
            MARKET,
            AccountId::new(ALICE.address()),
            AccountId::new(BOB.address()),
            100,
            10,
            1_000,
            1,
        );

        assert_ne!(vec![trade].hash(), vec![].hash());
    }
}
