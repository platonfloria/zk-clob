use std::sync::LazyLock;

use alloy_primitives::{Address, B256, address, keccak256};
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchInput, Deposit, DomainSha256Hash, ExchangeConfig,
    ExchangeId, FeeConfig, ForcedWithdrawal, MarketConfig, MarketId, MarketOrderBook, Order, SequencedOrder, Side,
    SignableOperation as _, Signature, SignedOrder, SignedWithdrawal, SigningDomain, State, Withdrawal,
};

#[derive(Clone, Copy)]
pub struct TestSigner {
    id: AccountId,
    secret_key: [u8; 32],
}

impl TestSigner {
    pub fn new(secret_key: [u8; 32]) -> Self {
        let key = SecretKey::from_byte_array(&secret_key).expect("fixture secret key must be valid");
        let public_key = PublicKey::from_secret_key(&Secp256k1::new(), &key).serialize_uncompressed();
        let public_key_hash = keccak256(&public_key[1..]);
        let id = AccountId::new(Address::from_slice(&public_key_hash[12..]));

        Self { id, secret_key }
    }

    pub const fn id(self) -> AccountId {
        self.id
    }

    pub const fn address(self) -> Address {
        *self.id.address()
    }

    pub fn sign(self, order: Order) -> SignedOrder {
        let secret_key = SecretKey::from_byte_array(&self.secret_key).expect("fixture secret key must be valid");
        let domain_hash = SIGNING_DOMAIN.hash();
        let signature =
            Secp256k1::new().sign_ecdsa_recoverable(&Message::from_digest(order.digest(&domain_hash)), &secret_key);
        let (recovery_id, compact) = signature.serialize_compact();
        SignedOrder::new(
            order,
            self.id,
            Signature::new(
                compact[..32].try_into().expect("r is 32 bytes"),
                compact[32..].try_into().expect("s is 32 bytes"),
                i32::from(recovery_id).try_into().expect("recovery ID fits in u8"),
            ),
        )
    }

    pub fn order(
        &self,
        market: MarketId,
        side: Side,
        price: u128,
        quantity: u128,
        nonce: u64,
        sequence: u64,
    ) -> SequencedOrder {
        self.sign(Order::new(market, side, price, quantity, nonce))
            .with_sequence(sequence)
    }

    pub fn account(&self, balances: Vec<AssetBalance>) -> Account {
        Account::new(self.id(), balances, 0)
    }

    pub fn withdrawal(&self, asset: AssetId, amount: u128, recipient: AccountId, nonce: u64) -> SignedWithdrawal {
        let withdrawal = Withdrawal::new(asset, amount, recipient, nonce);
        let secret_key = SecretKey::from_byte_array(&self.secret_key).expect("fixture secret key must be valid");
        let domain_hash = SIGNING_DOMAIN.hash();
        let signature = Secp256k1::new()
            .sign_ecdsa_recoverable(&Message::from_digest(withdrawal.digest(&domain_hash)), &secret_key);
        let (recovery_id, compact) = signature.serialize_compact();
        SignedWithdrawal::new(
            withdrawal,
            self.id,
            Signature::new(
                compact[..32].try_into().expect("r is 32 bytes"),
                compact[32..].try_into().expect("s is 32 bytes"),
                i32::from(recovery_id).try_into().expect("recovery ID fits in u8"),
            ),
        )
    }
}

const fn secret_key(value: u8) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[31] = value;
    bytes
}

pub const ETH: AssetConfig = AssetConfig::new(AssetId::new(Address::ZERO), 10u128.pow(18));
pub const USDC: AssetConfig = AssetConfig::new(
    AssetId::new(address!("0202020202020202020202020202020202020202")),
    10u128.pow(6),
);
pub const BTC: AssetConfig = AssetConfig::new(
    AssetId::new(address!("0303030303030303030303030303030303030303")),
    10u128.pow(8),
);
pub const ETH_USDC: MarketId = MarketId::new(B256::new([3; 32]));
pub const BTC_USDC: MarketId = MarketId::new(B256::new([4; 32]));
pub static ALICE: LazyLock<TestSigner> = LazyLock::new(|| TestSigner::new(secret_key(1)));
pub static BOB: LazyLock<TestSigner> = LazyLock::new(|| TestSigner::new(secret_key(2)));
pub static TREASURY: LazyLock<TestSigner> = LazyLock::new(|| TestSigner::new(secret_key(3)));
pub static CAROL: LazyLock<TestSigner> = LazyLock::new(|| TestSigner::new(secret_key(4)));
pub static DAVE: LazyLock<TestSigner> = LazyLock::new(|| TestSigner::new(secret_key(5)));
pub const EXCHANGE: ExchangeId = address!("2e234dae75c793f67a35089c9d99245e1c58470b");
pub const SIGNING_DOMAIN: SigningDomain = SigningDomain::new(1, 31_337, EXCHANGE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_expected_accounts_from_secret_keys() {
        assert_eq!(
            ALICE.id(),
            AccountId::new(address!("7e5f4552091a69125d5dfcb7b8c2659029395bdf"))
        );
        assert_eq!(
            BOB.id(),
            AccountId::new(address!("2b5ad5c4795c026514f8317c7a215e218dccd6cf"))
        );
        assert_eq!(
            TREASURY.id(),
            AccountId::new(address!("6813eb9362372eef6200f3b1dbc3f819671cba69"))
        );
        assert_eq!(
            CAROL.id(),
            AccountId::new(address!("1eff47bc3a10a45d4b230b5d10e37751fe6aa718"))
        );
    }
}

pub fn happy_path_fixture() -> BatchInput {
    let accounts = vec![
        ALICE.account(vec![AssetBalance::new(*USDC.id(), 10_000 * USDC.scale())]),
        BOB.account(vec![AssetBalance::new(*ETH.id(), ETH.scale())]),
        TREASURY.account(vec![]),
        CAROL.account(vec![AssetBalance::new(*USDC.id(), 50 * USDC.scale())]),
    ];
    let state = State::new(accounts);
    let old_state_root = state.root();
    let state = state.witness().expect("full-state witness should be valid");
    let account_index = |id: AccountId| -> u32 {
        state
            .accounts()
            .iter()
            .position(|account| *account.id() == id)
            .expect("fixture account must be present in witness") as u32
    };

    let orders = vec![
        ALICE
            .order(ETH_USDC, Side::Buy, 3_500 * USDC.scale(), ETH.scale(), 0, 1)
            .with_account_index(account_index(ALICE.id())),
        BOB.order(ETH_USDC, Side::Sell, 3_500 * USDC.scale(), ETH.scale(), 0, 2)
            .with_account_index(account_index(BOB.id())),
    ];
    let deposits = vec![Deposit::new(0, ALICE.id(), *ETH.id(), ETH.scale()).with_account_index(account_index(ALICE.id()))];
    let forced_withdrawals = vec![
        ForcedWithdrawal::new(0, CAROL.id(), *USDC.id(), 20 * USDC.scale())
            .with_account_index(account_index(CAROL.id())),
    ];
    let withdrawals = vec![
        ALICE
            .withdrawal(*USDC.id(), 100 * USDC.scale(), ALICE.id(), 1)
            .with_account_index(account_index(ALICE.id())),
    ];
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY.id(), 10),
    );
    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        state,
        0,
        deposits,
        0,
        forced_withdrawals,
        orders,
        withdrawals,
        vec![MarketOrderBook::new(ETH_USDC, vec![0], vec![1])],
        config,
    )
}

pub fn multi_market_happy_path_fixture() -> BatchInput {
    let accounts = vec![
        ALICE.account(vec![AssetBalance::new(*USDC.id(), 100_000 * USDC.scale())]),
        BOB.account(vec![AssetBalance::new(*ETH.id(), ETH.scale())]),
        TREASURY.account(vec![]),
        CAROL.account(vec![AssetBalance::new(*BTC.id(), BTC.scale())]),
    ];
    let state = State::new(accounts);
    let old_state_root = state.root();
    let state = state.witness().expect("full-state witness should be valid");
    let account_index = |id: AccountId| -> u32 {
        state
            .accounts()
            .iter()
            .position(|account| *account.id() == id)
            .expect("fixture account must be present in witness") as u32
    };
    let mut orders = Vec::with_capacity(20);

    for index in [3u64, 0, 4, 1, 2] {
        let buy_quantity = u128::from(index + 1) * ETH.scale() / 100;
        let sell_quantity = u128::from(5 - index) * ETH.scale() / 100;
        orders.push(
            ALICE
                .order(
                    ETH_USDC,
                    Side::Buy,
                    (3_600 - u128::from(index) * 50) * USDC.scale(),
                    buy_quantity,
                    index,
                    index * 2 + 1,
                )
                .with_account_index(account_index(ALICE.id())),
        );
        orders.push(
            BOB.order(
                ETH_USDC,
                Side::Sell,
                (3_300 + u128::from(index) * 25) * USDC.scale(),
                sell_quantity,
                index,
                index * 2 + 2,
            )
            .with_account_index(account_index(BOB.id())),
        );
    }

    for index in [2u64, 4, 0, 3, 1] {
        let buy_quantity = u128::from(index + 1) * BTC.scale() / 100;
        let sell_quantity = u128::from(5 - index) * BTC.scale() / 100;
        orders.push(
            ALICE
                .order(
                    BTC_USDC,
                    Side::Buy,
                    (62_000 - u128::from(index) * 500) * USDC.scale(),
                    buy_quantity,
                    index + 5,
                    index * 2 + 11,
                )
                .with_account_index(account_index(ALICE.id())),
        );
        orders.push(
            CAROL
                .order(
                    BTC_USDC,
                    Side::Sell,
                    (56_000 + u128::from(index) * 1_000) * USDC.scale(),
                    sell_quantity,
                    index,
                    index * 2 + 12,
                )
                .with_account_index(account_index(CAROL.id())),
        );
    }

    let config = ExchangeConfig::new(
        vec![ETH, USDC, BTC],
        vec![
            MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id()),
            MarketConfig::new(BTC_USDC, *BTC.id(), *USDC.id()),
        ],
        FeeConfig::new(TREASURY.id(), 10),
    );

    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        1,
        old_state_root,
        state,
        0,
        vec![],
        0,
        vec![],
        orders,
        vec![],
        vec![
            MarketOrderBook::new(ETH_USDC, vec![2, 6, 8, 0, 4], vec![3, 7, 9, 1, 5]),
            MarketOrderBook::new(BTC_USDC, vec![14, 18, 10, 16, 12], vec![15, 19, 11, 17, 13]),
        ],
        config,
    )
}
