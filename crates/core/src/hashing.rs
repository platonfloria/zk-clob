use sha2::{Digest as _, Sha256};

use crate::{
    Account, BatchHash, BatchMetadata, ConfigHash, ExchangeConfig, Order, StateRoot, Trade,
    TradesHash,
};

const DOMAIN_STATE: &[u8] = b"ZKCLOB_STATE_V1";
const DOMAIN_CONFIG: &[u8] = b"ZKCLOB_CONFIG_V1";
const DOMAIN_BATCH: &[u8] = b"ZKCLOB_BATCH_V1";
const DOMAIN_TRADES: &[u8] = b"ZKCLOB_TRADES_V1";

pub trait Sha256Hash {
    fn update_hash(&self, hasher: &mut Sha256);
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn compute_state_root(accounts: &[Account]) -> StateRoot {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_STATE);
    hasher.update((accounts.len() as u64).to_be_bytes());

    for account in accounts {
        account.update_hash(&mut hasher);
    }

    StateRoot::new(hasher.finalize().into())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn compute_config_hash(config: &ExchangeConfig) -> ConfigHash {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_CONFIG);
    config.update_hash(&mut hasher);
    ConfigHash::new(hasher.finalize().into())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn compute_batch_hash(
    metadata: &BatchMetadata,
    old_state_root: &StateRoot,
    config_hash: &ConfigHash,
    orders: &[Order],
) -> BatchHash {
    let mut orders: Vec<_> = orders.iter().collect();
    orders.sort_unstable_by(|a, b| {
        a.market_id()
            .cmp(b.market_id())
            .then_with(|| a.sequence().cmp(&b.sequence()))
    });

    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_BATCH);
    metadata.update_hash(&mut hasher);
    hasher.update(old_state_root);
    hasher.update(config_hash);
    hasher.update((orders.len() as u64).to_be_bytes());
    for order in orders {
        order.update_hash(&mut hasher);
    }
    BatchHash::new(hasher.finalize().into())
}

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn compute_trades_hash(trades: &[Trade]) -> TradesHash {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_TRADES);
    hasher.update((trades.len() as u64).to_be_bytes());
    for trade in trades {
        trade.update_hash(&mut hasher);
    }
    TradesHash::new(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use alloy_primitives::{Address, B256};

    use super::*;
    use crate::{
        AccountId, AssetBalance, AssetConfig, AssetId, ExchangeId, FeeConfig, MarketConfig,
        MarketId, Side,
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

        assert_ne!(
            compute_config_hash(&config(10)),
            compute_config_hash(&config(11))
        );
    }

    #[test]
    fn changing_an_order_changes_the_batch_hash() {
        let metadata = BatchMetadata::new(1, 31_337, ExchangeId::new([4; 32]), 0);
        let old_state_root = StateRoot::new([7; 32]);
        let config_hash = ConfigHash::new([8; 32]);
        let order = |price| Order::new(ALICE, MARKET, Side::Buy, price, ETH.scale(), 0, 1);

        assert_ne!(
            compute_batch_hash(&metadata, &old_state_root, &config_hash, &[order(100)]),
            compute_batch_hash(&metadata, &old_state_root, &config_hash, &[order(101)])
        );
    }

    #[test]
    fn changing_the_trade_list_changes_the_trades_hash() {
        let trade = Trade::new(MARKET, ALICE, BOB, 100, 10, 1_000, 1);

        assert_ne!(compute_trades_hash(&[trade]), compute_trades_hash(&[]));
    }
}
