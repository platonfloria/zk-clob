use alloy_primitives::b256;
use zk_clob_core::{
    Account, AssetBalance, AssetId, BatchInput, BatchOutput, ExchangeConfig, FeeConfig, MarketConfig, MarketOrderBook,
    Order, SequencedOrder, SettlementError, Side, SignedOrder, State, StateRoot, settle_batch,
};
use zk_clob_test_utils::{
    ALICE, BOB, CAROL, ETH, ETH_USDC, EXCHANGE, TREASURY, TestSigner, USDC, happy_path_fixture,
    multi_market_happy_path_fixture,
};
const PRICE: u128 = 3_500_000_000;
const BUYER_FEE_BPS: u16 = 10;

fn buy(trader: &TestSigner, price: u128, quantity: u128, nonce: u64, sequence: u64) -> SequencedOrder {
    trader.order(ETH_USDC, Side::Buy, price, quantity, nonce, sequence)
}

fn sell(trader: &TestSigner, price: u128, quantity: u128, nonce: u64, sequence: u64) -> SequencedOrder {
    trader.order(ETH_USDC, Side::Sell, price, quantity, nonce, sequence)
}

fn batch(
    accounts: Vec<Account>,
    orders: Vec<SequencedOrder>,
    buy_indices: Vec<u32>,
    sell_indices: Vec<u32>,
    buyer_fee_bps: u16,
) -> BatchInput {
    let state = State::new(accounts);
    let old_state_root = state.root();
    let state = state.witness().expect("full-state witness should be valid");
    let order_books = if buy_indices.is_empty() && sell_indices.is_empty() {
        vec![]
    } else {
        vec![MarketOrderBook::new(ETH_USDC, buy_indices, sell_indices)]
    };
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY.id(), buyer_fee_bps),
    );

    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        state,
        0,
        vec![],
        orders,
        vec![],
        order_books,
        config,
    )
}

fn settlement_error(input: BatchInput) -> SettlementError {
    match settle_batch(input) {
        Ok(_) => panic!("settlement should fail"),
        Err(error) => error,
    }
}

fn output_account<'a>(output: &'a BatchOutput, signer: &TestSigner) -> &'a Account {
    let id = signer.id();
    output
        .updated_accounts()
        .iter()
        .find(|account| account.id() == &id)
        .expect("account must remain in state")
}

fn balance(available: u128, asset: AssetId) -> AssetBalance {
    AssetBalance::new(asset, available)
}

#[test]
fn settles_one_full_fill_and_credits_the_buyer_fee() {
    let input = happy_path_fixture();
    let expected_old_state_root = input.expected_old_state_root;
    let output = match settle_batch(input) {
        Ok(output) => output,
        Err(error) => {
            panic!("happy-path settlement should succeed, got {error:?}")
        }
    };

    let account = |signer: &TestSigner| {
        let id = signer.id();
        output
            .updated_accounts()
            .iter()
            .find(|account| account.id() == &id)
            .expect("account must remain in state")
    };

    assert_eq!(account(&ALICE).balance(&ETH.id()), 2 * ETH.scale());
    assert_eq!(account(&ALICE).balance(&USDC.id()), 6_396_500_000);
    assert_eq!(account(&ALICE).next_nonce(), 2);
    assert_eq!(account(&BOB).balance(&ETH.id()), 0);
    assert_eq!(account(&BOB).balance(&USDC.id()), 3_500_000_000);
    assert_eq!(account(&TREASURY).balance(&USDC.id()), 3_500_000);

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(output.trades()[0].quote_amount(), 3_500_000_000);
    assert_eq!(output.trades()[0].quote_fee(), 3_500_000);

    assert_eq!(output.withdrawals().len(), 1);
    assert_eq!(output.withdrawals()[0].account(), &ALICE.id());
    assert_eq!(output.withdrawals()[0].recipient(), &ALICE.id());
    assert_eq!(output.withdrawals()[0].asset(), USDC.id());
    assert_eq!(output.withdrawals()[0].amount(), 100 * USDC.scale());

    let public = output.public();
    assert_eq!(public.oldDepositCursor, 0);
    assert_eq!(public.newDepositCursor, 1);
    assert_eq!(public.oldStateRoot, expected_old_state_root);
    assert_eq!(
        public.newStateRoot,
        State::new(output.updated_accounts().to_vec()).root()
    );

    assert_eq!(
        public.newStateRoot,
        b256!("53a12b7ff3e5e77eb159056cc76e69527e0e4d165064125cc66efbd1cb546d47")
    );
    assert_eq!(
        public.configHash,
        b256!("aa4416782ab2fdc4cdfd8fdfe430ae834ea47a50353b2a9fdf1a232c554aab94")
    );
    assert_eq!(
        public.batchHash,
        b256!("c5e70a05503dd7904bd27c23bb05fed8a6b148bc0febfa10f203d129d027a8e6")
    );
    assert_eq!(
        public.tradesHash,
        b256!("9f58c6a39c911fae55af47fe09d1a9dd0e0f1fa5c2ef43937e3cecde662b3c6e")
    );
}

#[test]
fn settles_twenty_orders_across_two_markets() {
    let output = settle_batch(multi_market_happy_path_fixture()).expect("multi-market settlement should succeed");

    assert_eq!(output.trades().len(), 18);
}

#[test]
fn partially_fills_buy_order() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, 2 * ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("partial buy fill should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(output_account(&output, &ALICE).balance(ETH.id()), ETH.scale());
}

#[test]
fn partially_fills_sell_order() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(2 * ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&BOB, PRICE, 2 * ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("partial sell fill should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(output_account(&output, &BOB).balance(ETH.id()), ETH.scale());
}

#[test]
fn fills_one_buy_from_multiple_sells() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
            CAROL.account(vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(&ALICE, PRICE, 2 * ETH.scale(), 0, 3),
            sell(&BOB, PRICE, ETH.scale(), 0, 1),
            sell(&CAROL, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1, 2],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("multiple fills should succeed");

    assert_eq!(output.trades().len(), 2);
    assert_eq!(
        output.trades().iter().map(|trade| trade.quantity()).sum::<u128>(),
        2 * ETH.scale()
    );
    assert_eq!(output_account(&output, &BOB).balance(ETH.id()), 0);
    assert_eq!(output_account(&output, &CAROL).balance(ETH.id()), 0);
}

#[test]
fn leaves_non_crossing_orders_unfilled() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, 3_400 * USDC.scale(), ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("non-crossing batch should succeed");

    assert!(output.trades().is_empty());
    assert_eq!(output_account(&output, &ALICE).balance(ETH.id()), 0);
    assert_eq!(output_account(&output, &BOB).balance(ETH.id()), ETH.scale());
}

#[test]
fn gives_better_price_priority() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
            CAROL.account(vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(&ALICE, 3_600 * USDC.scale(), ETH.scale(), 0, 3),
            sell(&BOB, PRICE, ETH.scale(), 0, 1),
            sell(&CAROL, 3_400 * USDC.scale(), ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![2, 1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("price-priority settlement should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quote_amount(), 3_400 * USDC.scale());
    assert_eq!(output_account(&output, &BOB).balance(ETH.id()), ETH.scale());
    assert_eq!(output_account(&output, &CAROL).balance(ETH.id()), 0);
}

#[test]
fn gives_earlier_sequence_time_priority() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
            CAROL.account(vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 3),
            sell(&BOB, PRICE, ETH.scale(), 0, 1),
            sell(&CAROL, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1, 2],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("time-priority settlement should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output_account(&output, &BOB).balance(ETH.id()), 0);
    assert_eq!(output_account(&output, &CAROL).balance(ETH.id()), ETH.scale());
}

#[test]
fn rejects_insufficient_base_balance() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account,
            asset,
            available: 0,
            required,
        } if account == BOB.id() && asset == *ETH.id() && required == ETH.scale()
    ));
}

#[test]
fn rejects_insufficient_quote_balance() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(PRICE - 1, *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account,
            asset,
            available,
            required: PRICE,
        } if account == ALICE.id() && asset == *USDC.id() && available == PRICE - 1
    ));
}

#[test]
fn includes_fee_when_checking_quote_balance() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(PRICE, *USDC.id())]),
            BOB.account(vec![balance(ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account,
            asset,
            available: PRICE,
            required,
        } if account == ALICE.id() && asset == *USDC.id() && required == PRICE + 3_500_000
    ));
}

#[test]
fn rejects_repeated_nonce() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            buy(&ALICE, PRICE - 1, ETH.scale(), 0, 2),
        ],
        vec![0, 1],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(settlement_error(input), SettlementError::InvalidNonce));
}

#[test]
fn rejects_skipped_nonce() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            TREASURY.account(vec![]),
        ],
        vec![buy(&ALICE, PRICE, ETH.scale(), 1, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(settlement_error(input), SettlementError::InvalidNonce));
}

#[test]
fn rejects_order_signed_by_another_trader() {
    let bob_signature = *BOB.order(ETH_USDC, Side::Buy, PRICE, ETH.scale(), 0, 1).signature();
    let order = SignedOrder::new(
        Order::new(ETH_USDC, Side::Buy, PRICE, ETH.scale(), 0),
        ALICE.id(),
        bob_signature,
    )
    .with_sequence(1);
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            TREASURY.account(vec![]),
        ],
        vec![order],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InvalidOrderSignature
    ));
}

#[test]
fn rejects_wrong_old_state_root() {
    let mut input = happy_path_fixture();
    input.expected_old_state_root = StateRoot::new([0xff; 32]);

    assert!(matches!(settlement_error(input), SettlementError::OldStateRootMismatch));
}

#[test]
fn rejects_duplicate_account() {
    let accounts = vec![
        ALICE.account(vec![balance(PRICE, *USDC.id())]),
        TREASURY.account(vec![]),
    ];
    let state = State::new(accounts);
    let old_state_root = state.root();
    let mut state = state.witness().expect("full-state witness should be valid");
    state
        .accounts_mut()
        .push(ALICE.account(vec![balance(ETH.scale(), *ETH.id())]));
    state.accounts_mut().sort_unstable_by_key(|account| *account.id());
    let input = BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        state,
        0,
        vec![],
        vec![],
        vec![],
        vec![],
        ExchangeConfig::new(
            vec![ETH, USDC],
            vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
            FeeConfig::new(TREASURY.id(), BUYER_FEE_BPS),
        ),
    );

    assert!(matches!(settlement_error(input), SettlementError::DuplicateAccount));
}

#[test]
fn rejects_arithmetic_overflow() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(u128::MAX, *USDC.id())]),
            BOB.account(vec![balance(2, *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![buy(&ALICE, u128::MAX, 2, 0, 1), sell(&BOB, u128::MAX, 2, 0, 2)],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(settlement_error(input), SettlementError::ArithmeticOverflow));
}

#[test]
fn conserves_every_asset() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            BOB.account(vec![balance(2 * ETH.scale(), *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("conserving settlement should succeed");
    let total = |asset: &AssetId| {
        output
            .updated_accounts()
            .iter()
            .map(|account| account.balance(asset))
            .sum::<u128>()
    };

    assert_eq!(total(ETH.id()), 2 * ETH.scale());
    assert_eq!(total(USDC.id()), 10_000 * USDC.scale());
}

#[test]
fn rejects_self_trade() {
    let input = batch(
        vec![
            ALICE.account(vec![
                balance(ETH.scale(), *ETH.id()),
                balance(10_000 * USDC.scale(), *USDC.id()),
            ]),
            TREASURY.account(vec![]),
        ],
        vec![
            buy(&ALICE, PRICE, ETH.scale(), 0, 1),
            sell(&ALICE, PRICE, ETH.scale(), 1, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    assert!(matches!(settlement_error(input), SettlementError::SelfTrade));
}

#[test]
fn rejects_zero_quantity() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(PRICE, *USDC.id())]),
            TREASURY.account(vec![]),
        ],
        vec![buy(&ALICE, PRICE, 0, 0, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(settlement_error(input), SettlementError::ZeroQuantity));
}

#[test]
fn rejects_zero_price() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(PRICE, *USDC.id())]),
            TREASURY.account(vec![]),
        ],
        vec![buy(&ALICE, 0, ETH.scale(), 0, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(settlement_error(input), SettlementError::ZeroPrice));
}

#[test]
fn rejects_quote_amount_that_rounds_to_zero() {
    let input = batch(
        vec![
            ALICE.account(vec![balance(1, *USDC.id())]),
            BOB.account(vec![balance(1, *ETH.id())]),
            TREASURY.account(vec![]),
        ],
        vec![buy(&ALICE, 1, 1, 0, 1), sell(&BOB, 1, 1, 0, 2)],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::TradeValueRoundsToZero
    ));
}
