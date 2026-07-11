use rstest::{fixture, rstest};
use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchHash, BatchInput, BatchMetadata,
    ConfigHash, ExchangeConfig, ExchangeId, FeeConfig, MarketConfig, MarketId, Order, Side,
    StateRoot, compute_batch_hash, compute_config_hash, compute_state_root, compute_trades_hash,
    settle_batch,
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
fn settlement_fixture(
    accounts: Vec<Account>,
    assets: Vec<AssetConfig>,
    market: MarketConfig,
) -> SettlementFixture {
    let metadata = BatchMetadata::new(1, 31_337, EXCHANGE, 0);
    let old_state_root = compute_state_root(&accounts);
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
    let config = ExchangeConfig::new(assets, vec![market], FeeConfig::new(TREASURY, 10));
    let config_hash = compute_config_hash(&config);
    let batch_hash = compute_batch_hash(&metadata, &old_state_root, &config_hash, &orders);
    let input = BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        accounts,
        orders,
        config,
    );
    SettlementFixture {
        input,
        old_state_root,
        config_hash,
        batch_hash,
    }
}

struct SettlementFixture {
    input: BatchInput,
    old_state_root: StateRoot,
    config_hash: ConfigHash,
    batch_hash: BatchHash,
}

#[rstest]
fn settles_one_full_fill_and_credits_the_buyer_fee(settlement_fixture: SettlementFixture) {
    let output = match settle_batch(settlement_fixture.input) {
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

    let public = output.public();
    assert_eq!(public.old_state_root(), &settlement_fixture.old_state_root);
    assert_eq!(
        public.new_state_root(),
        &compute_state_root(output.updated_accounts())
    );
    assert_eq!(public.config_hash(), &settlement_fixture.config_hash);
    assert_eq!(public.batch_hash(), &settlement_fixture.batch_hash);
    assert_eq!(public.trades_hash(), &compute_trades_hash(output.trades()));

    assert_eq!(
        public.new_state_root(),
        &StateRoot::new([
            107, 7, 24, 187, 151, 125, 4, 63, 13, 251, 59, 173, 6, 104, 232, 74, 165, 208, 25, 160,
            53, 100, 83, 99, 87, 216, 136, 53, 122, 24, 122, 207,
        ])
    );
    assert_eq!(
        public.config_hash(),
        &ConfigHash::new([
            151, 159, 133, 99, 97, 245, 106, 99, 149, 249, 241, 186, 179, 243, 115, 124, 114, 43,
            201, 166, 71, 146, 37, 160, 197, 155, 226, 18, 180, 199, 178, 200,
        ])
    );
    assert_eq!(
        public.batch_hash(),
        &BatchHash::new([
            57, 158, 4, 143, 24, 220, 232, 70, 226, 239, 202, 54, 138, 13, 41, 85, 111, 223, 159,
            26, 157, 155, 207, 151, 78, 150, 13, 189, 92, 70, 157, 125,
        ])
    );
    assert_eq!(
        public.trades_hash(),
        &zk_clob_core::TradesHash::new([
            166, 176, 135, 10, 100, 160, 94, 17, 161, 49, 100, 40, 246, 150, 98, 0, 32, 81, 72,
            224, 184, 106, 254, 13, 28, 30, 147, 29, 168, 227, 125, 62,
        ])
    );
}

#[test]
fn changing_a_balance_changes_the_state_root() {
    let original = vec![Account::new(
        ALICE,
        vec![AssetBalance::new(USDC.id(), 100)],
        0,
    )];
    let tampered = vec![Account::new(
        ALICE,
        vec![AssetBalance::new(USDC.id(), 101)],
        0,
    )];

    assert_ne!(compute_state_root(&original), compute_state_root(&tampered));
}

#[test]
fn changing_fee_config_changes_the_config_hash() {
    let config = |fee| {
        ExchangeConfig::new(
            vec![ETH, USDC],
            vec![MarketConfig::new(ETH_USDC, ETH.id(), USDC.id())],
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
    let metadata = BatchMetadata::new(1, 31_337, EXCHANGE, 0);
    let old_state_root = StateRoot::new([7; 32]);
    let config_hash = ConfigHash::new([8; 32]);
    let order = |price| Order::new(ALICE, ETH_USDC, Side::Buy, price, ETH.scale(), 0, 1);

    assert_ne!(
        compute_batch_hash(&metadata, &old_state_root, &config_hash, &[order(100)]),
        compute_batch_hash(&metadata, &old_state_root, &config_hash, &[order(101)])
    );
}

#[rstest]
fn changing_the_trade_list_changes_the_trades_hash(settlement_fixture: SettlementFixture) {
    let output = settle_batch(settlement_fixture.input).expect("settlement should succeed");

    assert_ne!(
        compute_trades_hash(output.trades()),
        compute_trades_hash(&[])
    );
}
