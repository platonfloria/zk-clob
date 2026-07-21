use zk_clob_core::{
    AssetBalance, BatchMetadata, ConsumedDepositsHash, Deposit, ExchangeConfig, FeeConfig,
    MarketConfig, Side, settle_batch,
};
use zk_clob_host::{AccountTree, BatchBuildError, BatchBuilder};
use zk_clob_test_utils::{ALICE, BOB, CAROL, ETH, ETH_USDC, EXCHANGE, TREASURY, USDC};

#[test]
fn builds_and_applies_a_subset_account_witness() {
    let (carol, treasury) = (CAROL.id(), TREASURY.id());
    let mut state = AccountTree::new(vec![
        ALICE.account(vec![AssetBalance::new(*USDC.id(), 10_000 * USDC.scale())]),
        BOB.account(vec![AssetBalance::new(*ETH.id(), ETH.scale())]),
        CAROL.account(vec![AssetBalance::new(*USDC.id(), USDC.scale())]),
        TREASURY.account(vec![]),
    ])
    .expect("account tree should be valid");
    let old_root = state.root();
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(treasury, 10),
    );
    let mut builder = BatchBuilder::new(
        &state,
        &config,
        BatchMetadata::new(1, 31_337, EXCHANGE, 0),
        0,
    );
    assert!(matches!(
        builder.order(ALICE.order(ETH_USDC, Side::Buy, 0, ETH.scale(), 0, 1)),
        Err(BatchBuildError::ZeroPrice)
    ));
    builder
        .order(BOB.order(
            ETH_USDC,
            Side::Sell,
            3_500 * USDC.scale(),
            ETH.scale(),
            0,
            2,
        ))
        .expect("sell order should be accepted");
    builder
        .order(ALICE.order(ETH_USDC, Side::Buy, 3_500 * USDC.scale(), ETH.scale(), 0, 1))
        .expect("buy order should be accepted");
    let input = builder.build().expect("batch should build");
    let output = settle_batch(input).expect("built batch should settle");

    assert_eq!(output.public().oldStateRoot, old_root);
    state
        .apply(output.updated_accounts().to_vec())
        .expect("settled accounts should update existing leaves");
    assert_eq!(state.root(), output.public().newStateRoot);
    assert_eq!(
        state.account(&carol).unwrap().balance(USDC.id()),
        USDC.scale()
    );
}

#[test]
fn deposits_create_accounts_and_advance_the_cursor() {
    let (carol, treasury) = (CAROL.id(), TREASURY.id());
    let mut state =
        AccountTree::new(vec![TREASURY.account(vec![])]).expect("account tree should be valid");
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(treasury, 10),
    );
    let mut builder = BatchBuilder::new(
        &state,
        &config,
        BatchMetadata::new(1, 31_337, EXCHANGE, 1),
        7,
    );
    builder
        .deposit(Deposit::new(7, carol, *USDC.id(), 500 * USDC.scale()))
        .expect("deposit should be accepted");

    let output = settle_batch(builder.build().expect("deposit batch should build"))
        .expect("deposit batch should settle");

    assert_eq!(output.public().oldDepositCursor, 7);
    assert_eq!(output.public().newDepositCursor, 8);
    assert_ne!(
        output.public().consumedDepositsHash,
        ConsumedDepositsHash::ZERO
    );
    state
        .apply(output.updated_accounts().to_vec())
        .expect("new account should be inserted into the host tree");
    assert_eq!(
        state.account(&carol).unwrap().balance(USDC.id()),
        500 * USDC.scale()
    );
}
