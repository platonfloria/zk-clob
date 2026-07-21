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
    ($name:literal, $body:block) => {{
        println!(concat!("cycle-tracker-start: ", $name));
        let result = $body;
        println!(concat!("cycle-tracker-end: ", $name));
        result
    }};

    ($name:literal, $($body:tt)*) => {
        println!(concat!("cycle-tracker-start: ", $name));
        $($body)*
        println!(concat!("cycle-tracker-end: ", $name));
    };
}

#[cfg(not(feature = "sp1-cycle-tracking"))]
macro_rules! cycle_tracker {
    ($name:literal, $body:block) => {
        $body
    };

    ($name:literal, $($body:tt)*) => {
        $($body)*
    };
}

mod consts;
mod errors;
mod hashing;
mod matching;
mod settlement;
mod state;
mod trees;
mod types;
mod validation;

pub use consts::{MAX_DEPOSITS_PER_BATCH, MAX_ORDERS_PER_BATCH, MAX_TOUCHED_ACCOUNTS_PER_BATCH};
pub use errors::SettlementError;
pub use hashing::DomainSha256Hash;
pub use settlement::settle_batch;
pub use state::{State, StateWitness};
pub use types::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchHash, BatchInput, BatchMetadata, BatchOutput,
    ConfigHash, ConsumedDepositsHash, Deposit, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig, MarketId,
    MarketOrderBook, Order, OrderSignature, PublicOutput, SequencedOrder, Side, SignedOrder, StateRoot, Trade,
    TradesHash,
};
