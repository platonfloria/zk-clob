// SP1 6.3.1's `cycle_tracker` attribute emits tracker commands with
// `eprintln!`, while its profiling executor parses commands from stdout.
// Keep the attribute usable until those two components agree on the stream.
#[cfg(feature = "sp1-cycle-tracking")]
macro_rules! eprintln {
    ($($arg:tt)*) => {
        println!($($arg)*)
    };
}

#[cfg(feature = "sp1-cycle-tracking")]
macro_rules! cycle_tracker {
    ($name:literal, { $($body:tt)* }) => {
        println!(concat!("cycle-tracker-start: ", $name));
        $($body)*
        println!(concat!("cycle-tracker-end: ", $name));
    };

    ($name:literal, return $body:block) => {{
        println!(concat!("cycle-tracker-start: ", $name));
        let result = $body;
        println!(concat!("cycle-tracker-end: ", $name));
        result
    }};
}

#[cfg(not(feature = "sp1-cycle-tracking"))]
macro_rules! cycle_tracker {
    ($name:literal, { $($body:tt)* }) => {
        $($body)*
    };

    ($name:literal, return $body:block) => {
        $body
    };
}

mod consts;
mod errors;
mod hashing;
mod matching;
mod settlement;
mod types;
mod validation;

pub use errors::SettlementError;
pub use hashing::compute_state_root;
pub use settlement::settle_batch;
pub use types::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchHash, BatchInput, BatchMetadata,
    BatchOutput, ConfigHash, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig, MarketId,
    MarketOrderBook, Order, PublicOutput, Side, StateRoot, Trade, TradesHash,
};
