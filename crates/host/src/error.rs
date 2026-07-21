use std::{error::Error, fmt};

use zk_clob_core::{AccountId, AssetId, MarketId};

#[derive(Debug)]
pub enum BatchBuildError {
    AccountIndexOverflow,
    DuplicateAccount(AccountId),
    DuplicateNonce(AccountId, u64),
    DuplicateSequence(u64),
    DepositCursorOverflow,
    InvalidDepositCursor { expected: u64, actual: u64 },
    InvalidNonce(AccountId),
    InvalidStateProof,
    OrderIndexOverflow,
    TooManyAccounts,
    TooManyDeposits,
    TooManyOrders,
    UnknownAccount(AccountId),
    UnknownAsset(AssetId),
    UnknownMarket(MarketId),
    ZeroPrice,
    ZeroQuantity,
    ZeroDepositAmount,
}

impl fmt::Display for BatchBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccountIndexOverflow => formatter.write_str("account index does not fit in u32"),
            Self::DuplicateAccount(account) => {
                write!(formatter, "duplicate account: {account:?}")
            }
            Self::DuplicateNonce(account, nonce) => {
                write!(formatter, "duplicate nonce {nonce} for account {account:?}")
            }
            Self::DuplicateSequence(sequence) => {
                write!(formatter, "duplicate order sequence: {sequence}")
            }
            Self::DepositCursorOverflow => formatter.write_str("deposit cursor overflow"),
            Self::InvalidDepositCursor { expected, actual } => {
                write!(
                    formatter,
                    "invalid deposit cursor: expected {expected}, got {actual}"
                )
            }
            Self::InvalidNonce(account) => {
                write!(formatter, "invalid nonce for account: {account:?}")
            }
            Self::InvalidStateProof => formatter.write_str("failed to build state multiproof"),
            Self::OrderIndexOverflow => formatter.write_str("order index does not fit in u32"),
            Self::TooManyAccounts => formatter.write_str("too many touched accounts in batch"),
            Self::TooManyDeposits => formatter.write_str("too many deposits in batch"),
            Self::TooManyOrders => formatter.write_str("too many orders in batch"),
            Self::UnknownAccount(account) => {
                write!(formatter, "unknown account: {account:?}")
            }
            Self::UnknownAsset(asset) => {
                write!(formatter, "unknown asset: {asset:?}")
            }
            Self::UnknownMarket(market) => {
                write!(formatter, "unknown market: {market:?}")
            }
            Self::ZeroPrice => formatter.write_str("order price must be positive"),
            Self::ZeroQuantity => formatter.write_str("order quantity must be positive"),
            Self::ZeroDepositAmount => formatter.write_str("deposit amount must be positive"),
        }
    }
}

impl Error for BatchBuildError {}
