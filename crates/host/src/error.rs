use std::{error::Error, fmt};

use zk_clob_core::{AccountId, MarketId};

#[derive(Debug)]
pub enum BatchBuildError {
    AccountIndexOverflow,
    DuplicateAccount(AccountId),
    DuplicateNonce(AccountId, u64),
    DuplicateSequence(u64),
    InvalidNonce(AccountId),
    InvalidStateProof,
    OrderIndexOverflow,
    TooManyAccounts,
    TooManyOrders,
    UnknownAccount(AccountId),
    UnknownMarket(MarketId),
    ZeroPrice,
    ZeroQuantity,
}

impl fmt::Display for BatchBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountIndexOverflow => formatter.write_str("account index does not fit in u32"),
            Self::DuplicateAccount(account) => write!(formatter, "duplicate account: {account:?}"),
            Self::DuplicateNonce(account, nonce) => {
                write!(formatter, "duplicate nonce {nonce} for account {account:?}")
            }
            Self::DuplicateSequence(sequence) => {
                write!(formatter, "duplicate order sequence: {sequence}")
            }
            Self::InvalidNonce(account) => {
                write!(formatter, "invalid nonce for account: {account:?}")
            }
            Self::InvalidStateProof => formatter.write_str("failed to build state multiproof"),
            Self::OrderIndexOverflow => formatter.write_str("order index does not fit in u32"),
            Self::TooManyAccounts => formatter.write_str("too many touched accounts in batch"),
            Self::TooManyOrders => formatter.write_str("too many orders in batch"),
            Self::UnknownAccount(account) => write!(formatter, "unknown account: {account:?}"),
            Self::UnknownMarket(market) => write!(formatter, "unknown market: {market:?}"),
            Self::ZeroPrice => formatter.write_str("order price must be positive"),
            Self::ZeroQuantity => formatter.write_str("order quantity must be positive"),
        }
    }
}

impl Error for BatchBuildError {}
