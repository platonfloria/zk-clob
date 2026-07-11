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

pub fn compute_state_root(accounts: &[Account]) -> StateRoot {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_STATE);
    hasher.update((accounts.len() as u64).to_be_bytes());

    for account in accounts {
        account.update_hash(&mut hasher);
    }

    StateRoot::new(hasher.finalize().into())
}

pub fn compute_config_hash(config: &ExchangeConfig) -> ConfigHash {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_CONFIG);
    config.update_hash(&mut hasher);
    ConfigHash::new(hasher.finalize().into())
}

pub fn compute_batch_hash(
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
    old_state_root.update_hash(&mut hasher);
    config_hash.update_hash(&mut hasher);
    hasher.update((orders.len() as u64).to_be_bytes());
    for order in orders {
        order.update_hash(&mut hasher);
    }
    BatchHash::new(hasher.finalize().into())
}

pub fn compute_trades_hash(trades: &[Trade]) -> TradesHash {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_TRADES);
    hasher.update((trades.len() as u64).to_be_bytes());
    for trade in trades {
        trade.update_hash(&mut hasher);
    }
    TradesHash::new(hasher.finalize().into())
}
