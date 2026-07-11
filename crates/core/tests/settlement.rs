use rstest::{fixture, rstest};
use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchInput, ExchangeConfig, ExchangeId,
    FeeConfig, MarketConfig, MarketId, Order, Side, compute_state_root, settle_batch,
};

const ETH: AssetConfig = AssetConfig::new(AssetId::new([1; 32]), 10u128.pow(18));
const USDC: AssetConfig = AssetConfig::new(AssetId::new([2; 32]), 10u128.pow(6));
const ETH_USDC: MarketId = MarketId::new([3; 32]);
const ALICE: AccountId = AccountId::new([1; 20]);
const BOB: AccountId = AccountId::new([2; 20]);
const TREASURY: AccountId = AccountId::new([3; 20]);
const EXCHANGE: ExchangeId = ExchangeId::new([4; 32]);

#[fixture]
fn market() -> MarketConfig {
    MarketConfig::new(ETH_USDC, ETH.id(), USDC.id())
}

#[fixture]
fn assets() -> Vec<AssetConfig> {
    vec![ETH, USDC]
}

#[fixture]
fn accounts() -> Vec<Account> {
    vec![
        Account::new(
            ALICE,
            vec![AssetBalance::new(USDC.id(), 10_000 * USDC.scale())],
            0,
        ),
        Account::new(BOB, vec![AssetBalance::new(ETH.id(), ETH.scale())], 0),
        Account::new(TREASURY, vec![], 0),
    ]
}

#[fixture]
fn batch_input(
    accounts: Vec<Account>,
    assets: Vec<AssetConfig>,
    market: MarketConfig,
) -> BatchInput {
    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        compute_state_root(&accounts),
        accounts,
        vec![
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
        ],
        ExchangeConfig::new(assets, vec![market], FeeConfig::new(TREASURY, 10)),
    )
}

#[rstest]
fn settles_one_full_fill_and_credits_the_buyer_fee(batch_input: BatchInput) {
    let output = match settle_batch(batch_input) {
        Ok(output) => output,
        Err(error) => panic!("happy-path settlement should succeed, got {error:?}"),
    };

    let account = |id: AccountId| {
        output
            .updated_accounts()
            .iter()
            .find(|account| account.id() == &id)
            .expect("account must remain in state")
    };

    assert_eq!(account(ALICE).balance(&ETH.id()), ETH.scale());
    assert_eq!(account(ALICE).balance(&USDC.id()), 6_496_500_000);
    assert_eq!(account(BOB).balance(&ETH.id()), 0);
    assert_eq!(account(BOB).balance(&USDC.id()), 3_500_000_000);
    assert_eq!(account(TREASURY).balance(&USDC.id()), 3_500_000);

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(output.trades()[0].quote_amount(), 3_500_000_000);
    assert_eq!(output.trades()[0].quote_fee(), 3_500_000);
}
