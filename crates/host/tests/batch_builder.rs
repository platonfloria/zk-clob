use zk_clob_core::{
    AssetBalance, BatchMetadata, ConsumedDepositsHash, Deposit, ExchangeConfig, FeeConfig, MarketConfig, Side,
    WithdrawalsHash, settle_batch,
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
    let mut builder = BatchBuilder::new(&state, &config, BatchMetadata::new(1, 31_337, EXCHANGE, 0), 0);
    assert!(matches!(
        builder.order(ALICE.order(ETH_USDC, Side::Buy, 0, ETH.scale(), 0, 1)),
        Err(BatchBuildError::ZeroPrice)
    ));
    builder
        .order(BOB.order(ETH_USDC, Side::Sell, 3_500 * USDC.scale(), ETH.scale(), 0, 2))
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
    assert_eq!(state.account(&carol).unwrap().balance(USDC.id()), USDC.scale());
}

#[test]
fn deposits_create_accounts_and_advance_the_cursor() {
    let (carol, treasury) = (CAROL.id(), TREASURY.id());
    let mut state = AccountTree::new(vec![TREASURY.account(vec![])]).expect("account tree should be valid");
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(treasury, 10),
    );
    let mut builder = BatchBuilder::new(&state, &config, BatchMetadata::new(1, 31_337, EXCHANGE, 1), 7);
    builder
        .deposit(Deposit::new(7, carol, *USDC.id(), 500 * USDC.scale()))
        .expect("deposit should be accepted");

    let output =
        settle_batch(builder.build().expect("deposit batch should build")).expect("deposit batch should settle");

    assert_eq!(output.public().oldDepositCursor, 7);
    assert_eq!(output.public().newDepositCursor, 8);
    assert_ne!(output.public().consumedDepositsHash, ConsumedDepositsHash::ZERO);
    state
        .apply(output.updated_accounts().to_vec())
        .expect("new account should be inserted into the host tree");
    assert_eq!(state.account(&carol).unwrap().balance(USDC.id()), 500 * USDC.scale());
}

#[test]
fn includes_a_signed_withdrawal_in_the_batch() {
    let state = AccountTree::new(vec![
        ALICE.account(vec![AssetBalance::new(*USDC.id(), 10 * USDC.scale())]),
        TREASURY.account(vec![]),
    ])
    .expect("account tree should be valid");
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY.id(), 10),
    );
    let mut builder = BatchBuilder::new(&state, &config, BatchMetadata::new(1, 31_337, EXCHANGE, 2), 0);
    builder
        .withdraw(ALICE.withdrawal(*USDC.id(), USDC.scale(), ALICE.id(), 0))
        .expect("withdrawal should be accepted");

    let input = builder.build().expect("batch should build");
    assert_eq!(input.withdrawals().len(), 1);
    assert!(input.withdrawals()[0].has_valid_signature());

    let output = settle_batch(input).expect("withdrawal batch should settle");
    let alice = output
        .updated_accounts()
        .iter()
        .find(|account| account.id() == &ALICE.id())
        .expect("Alice must remain in state");
    assert_eq!(alice.balance(USDC.id()), 9 * USDC.scale());
    assert_eq!(alice.next_nonce(), 1);
    assert_eq!(output.withdrawals().len(), 1);
    assert_eq!(output.withdrawals()[0].account(), &ALICE.id());
    assert_eq!(output.withdrawals()[0].recipient(), &ALICE.id());
    assert_eq!(output.withdrawals()[0].asset(), USDC.id());
    assert_eq!(output.withdrawals()[0].amount(), USDC.scale());
    assert_ne!(output.public().withdrawalsHash, WithdrawalsHash::ZERO);
}

#[test]
fn rejects_cumulative_withdrawals_above_the_committed_balance() {
    let state = AccountTree::new(vec![
        ALICE.account(vec![AssetBalance::new(*USDC.id(), 10 * USDC.scale())]),
        TREASURY.account(vec![]),
    ])
    .expect("account tree should be valid");
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY.id(), 10),
    );
    let mut builder = BatchBuilder::new(&state, &config, BatchMetadata::new(1, 31_337, EXCHANGE, 3), 0);
    builder
        .withdraw(ALICE.withdrawal(*USDC.id(), 6 * USDC.scale(), ALICE.id(), 0))
        .expect("first withdrawal should fit");

    assert!(matches!(
        builder.withdraw(ALICE.withdrawal(*USDC.id(), 5 * USDC.scale(), ALICE.id(), 1)),
        Err(BatchBuildError::InsufficientBalance {
            account,
            asset,
            available,
            required,
        }) if account == ALICE.id()
            && asset == *USDC.id()
            && available == 10 * USDC.scale()
            && required == 11 * USDC.scale()
    ));
}
