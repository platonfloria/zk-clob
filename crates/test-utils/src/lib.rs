use alloy_primitives::{Address, B256};
use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchInput, ExchangeConfig, ExchangeId,
    FeeConfig, MarketConfig, MarketId, MarketOrderBook, Order, Side, build_state_multiproof,
    compute_state_root,
};

pub const ETH: AssetConfig = AssetConfig::new(AssetId::new(B256::new([1; 32])), 10u128.pow(18));
pub const USDC: AssetConfig = AssetConfig::new(AssetId::new(B256::new([2; 32])), 10u128.pow(6));
pub const BTC: AssetConfig = AssetConfig::new(AssetId::new(B256::new([3; 32])), 10u128.pow(8));
pub const ETH_USDC: MarketId = MarketId::new(B256::new([3; 32]));
pub const BTC_USDC: MarketId = MarketId::new(B256::new([4; 32]));
pub const ALICE: AccountId = AccountId::new(Address::new([1; 20]));
pub const BOB: AccountId = AccountId::new(Address::new([2; 20]));
pub const TREASURY: AccountId = AccountId::new(Address::new([3; 20]));
pub const CAROL: AccountId = AccountId::new(Address::new([4; 20]));
pub const EXCHANGE: ExchangeId = ExchangeId::new([4; 32]);

pub fn happy_path_fixture() -> BatchInput {
    let accounts = vec![
        Account::new(
            ALICE,
            vec![AssetBalance::new(*USDC.id(), 10_000 * USDC.scale())],
            0,
        ),
        Account::new(BOB, vec![AssetBalance::new(*ETH.id(), ETH.scale())], 0),
        Account::new(TREASURY, vec![], 0),
    ];
    let old_state_root = compute_state_root(&accounts);
    let state_multiproof = build_state_multiproof(&accounts);
    let orders = vec![
        Order::new(
            ALICE,
            ETH_USDC,
            Side::Buy,
            3_500 * USDC.scale(),
            ETH.scale(),
            0,
            1,
        ),
        Order::new(
            BOB,
            ETH_USDC,
            Side::Sell,
            3_500 * USDC.scale(),
            ETH.scale(),
            0,
            2,
        ),
    ];
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY, 10),
    );
    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        accounts,
        state_multiproof,
        orders,
        vec![MarketOrderBook::new(ETH_USDC, vec![0], vec![1])],
        config,
    )
}

pub fn multi_market_happy_path_fixture() -> BatchInput {
    let accounts = vec![
        Account::new(
            ALICE,
            vec![AssetBalance::new(*USDC.id(), 100_000 * USDC.scale())],
            0,
        ),
        Account::new(BOB, vec![AssetBalance::new(*ETH.id(), ETH.scale())], 0),
        Account::new(TREASURY, vec![], 0),
        Account::new(CAROL, vec![AssetBalance::new(*BTC.id(), BTC.scale())], 0),
    ];
    let old_state_root = compute_state_root(&accounts);
    let state_multiproof = build_state_multiproof(&accounts);
    let mut orders = Vec::with_capacity(20);

    for index in [3u64, 0, 4, 1, 2] {
        let buy_quantity = u128::from(index + 1) * ETH.scale() / 100;
        let sell_quantity = u128::from(5 - index) * ETH.scale() / 100;
        orders.push(Order::new(
            ALICE,
            ETH_USDC,
            Side::Buy,
            (3_600 - u128::from(index) * 50) * USDC.scale(),
            buy_quantity,
            index,
            index * 2 + 1,
        ));
        orders.push(Order::new(
            BOB,
            ETH_USDC,
            Side::Sell,
            (3_300 + u128::from(index) * 25) * USDC.scale(),
            sell_quantity,
            index,
            index * 2 + 2,
        ));
    }

    for index in [2u64, 4, 0, 3, 1] {
        let buy_quantity = u128::from(index + 1) * BTC.scale() / 100;
        let sell_quantity = u128::from(5 - index) * BTC.scale() / 100;
        orders.push(Order::new(
            ALICE,
            BTC_USDC,
            Side::Buy,
            (62_000 - u128::from(index) * 500) * USDC.scale(),
            buy_quantity,
            index + 5,
            index * 2 + 11,
        ));
        orders.push(Order::new(
            CAROL,
            BTC_USDC,
            Side::Sell,
            (56_000 + u128::from(index) * 1_000) * USDC.scale(),
            sell_quantity,
            index,
            index * 2 + 12,
        ));
    }

    let config = ExchangeConfig::new(
        vec![ETH, USDC, BTC],
        vec![
            MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id()),
            MarketConfig::new(BTC_USDC, *BTC.id(), *USDC.id()),
        ],
        FeeConfig::new(TREASURY, 10),
    );

    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        1,
        old_state_root,
        accounts,
        state_multiproof,
        orders,
        vec![
            MarketOrderBook::new(ETH_USDC, vec![2, 6, 8, 0, 4], vec![3, 7, 9, 1, 5]),
            MarketOrderBook::new(BTC_USDC, vec![14, 18, 10, 16, 12], vec![15, 19, 11, 17, 13]),
        ],
        config,
    )
}
