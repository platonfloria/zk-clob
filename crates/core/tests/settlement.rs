use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetId, BatchHash, BatchInput, BatchOutput, ConfigHash,
    ExchangeConfig, FeeConfig, MarketConfig, MarketOrderBook, Order, SettlementError, Side,
    StateRoot, compute_state_root, settle_batch,
};
use zk_clob_test_utils::{
    ALICE, BOB, CAROL, ETH, ETH_USDC, EXCHANGE, TREASURY, USDC, happy_path_fixture,
    multi_market_happy_path_fixture,
};

const PRICE: u128 = 3_500_000_000;
const BUYER_FEE_BPS: u16 = 10;

fn account(id: AccountId, balances: Vec<AssetBalance>) -> Account {
    Account::new(id, balances, 0)
}

fn buy(trader: AccountId, price: u128, quantity: u128, nonce: u64, sequence: u64) -> Order {
    Order::new(
        trader,
        ETH_USDC,
        Side::Buy,
        price,
        quantity,
        nonce,
        sequence,
    )
}

fn sell(trader: AccountId, price: u128, quantity: u128, nonce: u64, sequence: u64) -> Order {
    Order::new(
        trader,
        ETH_USDC,
        Side::Sell,
        price,
        quantity,
        nonce,
        sequence,
    )
}

fn batch(
    accounts: Vec<Account>,
    orders: Vec<Order>,
    buy_indices: Vec<u32>,
    sell_indices: Vec<u32>,
    buyer_fee_bps: u16,
) -> BatchInput {
    let old_state_root = compute_state_root(&accounts);
    let order_books = if buy_indices.is_empty() && sell_indices.is_empty() {
        vec![]
    } else {
        vec![MarketOrderBook::new(ETH_USDC, buy_indices, sell_indices)]
    };
    let config = ExchangeConfig::new(
        vec![ETH, USDC],
        vec![MarketConfig::new(ETH_USDC, *ETH.id(), *USDC.id())],
        FeeConfig::new(TREASURY, buyer_fee_bps),
    );

    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        accounts,
        orders,
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

fn output_account(output: &BatchOutput, id: AccountId) -> &Account {
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
    assert_eq!(public.oldStateRoot, expected_old_state_root);
    assert_eq!(
        public.newStateRoot,
        compute_state_root(output.updated_accounts())
    );

    assert_eq!(
        public.newStateRoot,
        StateRoot::new([
            107, 7, 24, 187, 151, 125, 4, 63, 13, 251, 59, 173, 6, 104, 232, 74, 165, 208, 25, 160,
            53, 100, 83, 99, 87, 216, 136, 53, 122, 24, 122, 207,
        ])
    );
    assert_eq!(
        public.configHash,
        ConfigHash::new([
            151, 159, 133, 99, 97, 245, 106, 99, 149, 249, 241, 186, 179, 243, 115, 124, 114, 43,
            201, 166, 71, 146, 37, 160, 197, 155, 226, 18, 180, 199, 178, 200,
        ])
    );
    assert_eq!(
        public.batchHash,
        BatchHash::new([
            57, 158, 4, 143, 24, 220, 232, 70, 226, 239, 202, 54, 138, 13, 41, 85, 111, 223, 159,
            26, 157, 155, 207, 151, 78, 150, 13, 189, 92, 70, 157, 125,
        ])
    );
    assert_eq!(
        public.tradesHash,
        zk_clob_core::TradesHash::new([
            166, 176, 135, 10, 100, 160, 94, 17, 161, 49, 100, 40, 246, 150, 98, 0, 32, 81, 72,
            224, 184, 106, 254, 13, 28, 30, 147, 29, 168, 227, 125, 62,
        ])
    );
}

#[test]
fn settles_twenty_orders_across_two_markets() {
    let output = settle_batch(multi_market_happy_path_fixture())
        .expect("multi-market settlement should succeed");

    assert_eq!(output.trades().len(), 18);
}

#[test]
fn partially_fills_buy_order() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, 2 * ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("partial buy fill should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(
        output_account(&output, ALICE).balance(ETH.id()),
        ETH.scale()
    );
}

#[test]
fn partially_fills_sell_order() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(2 * ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(BOB, PRICE, 2 * ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("partial sell fill should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quantity(), ETH.scale());
    assert_eq!(output_account(&output, BOB).balance(ETH.id()), ETH.scale());
}

#[test]
fn fills_one_buy_from_multiple_sells() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
            account(CAROL, vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(ALICE, PRICE, 2 * ETH.scale(), 0, 3),
            sell(BOB, PRICE, ETH.scale(), 0, 1),
            sell(CAROL, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1, 2],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("multiple fills should succeed");

    assert_eq!(output.trades().len(), 2);
    assert_eq!(
        output
            .trades()
            .iter()
            .map(|trade| trade.quantity())
            .sum::<u128>(),
        2 * ETH.scale()
    );
    assert_eq!(output_account(&output, BOB).balance(ETH.id()), 0);
    assert_eq!(output_account(&output, CAROL).balance(ETH.id()), 0);
}

#[test]
fn leaves_non_crossing_orders_unfilled() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, 3_400 * USDC.scale(), ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("non-crossing batch should succeed");

    assert!(output.trades().is_empty());
    assert_eq!(output_account(&output, ALICE).balance(ETH.id()), 0);
    assert_eq!(output_account(&output, BOB).balance(ETH.id()), ETH.scale());
}

#[test]
fn gives_better_price_priority() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
            account(CAROL, vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(ALICE, 3_600 * USDC.scale(), ETH.scale(), 0, 3),
            sell(BOB, PRICE, ETH.scale(), 0, 1),
            sell(CAROL, 3_400 * USDC.scale(), ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![2, 1],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("price-priority settlement should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output.trades()[0].quote_amount(), 3_400 * USDC.scale());
    assert_eq!(output_account(&output, BOB).balance(ETH.id()), ETH.scale());
    assert_eq!(output_account(&output, CAROL).balance(ETH.id()), 0);
}

#[test]
fn gives_earlier_sequence_time_priority() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
            account(CAROL, vec![balance(ETH.scale(), *ETH.id())]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 3),
            sell(BOB, PRICE, ETH.scale(), 0, 1),
            sell(CAROL, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1, 2],
        BUYER_FEE_BPS,
    );

    let output = settle_batch(input).expect("time-priority settlement should succeed");

    assert_eq!(output.trades().len(), 1);
    assert_eq!(output_account(&output, BOB).balance(ETH.id()), 0);
    assert_eq!(
        output_account(&output, CAROL).balance(ETH.id()),
        ETH.scale()
    );
}

#[test]
fn rejects_insufficient_base_balance() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account: BOB,
            asset,
            available: 0,
            required,
        } if asset == *ETH.id() && required == ETH.scale()
    ));
}

#[test]
fn rejects_insufficient_quote_balance() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(PRICE - 1, *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account: ALICE,
            asset,
            available,
            required: PRICE,
        } if asset == *USDC.id() && available == PRICE - 1
    ));
}

#[test]
fn includes_fee_when_checking_quote_balance() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(PRICE, *USDC.id())]),
            account(BOB, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InsufficientBalance {
            account: ALICE,
            asset,
            available: PRICE,
            required,
        } if asset == *USDC.id() && required == PRICE + 3_500_000
    ));
}

#[test]
fn rejects_repeated_nonce() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            buy(ALICE, PRICE - 1, ETH.scale(), 0, 2),
        ],
        vec![0, 1],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InvalidNonce
    ));
}

#[test]
fn rejects_skipped_nonce() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(TREASURY, vec![]),
        ],
        vec![buy(ALICE, PRICE, ETH.scale(), 1, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::InvalidNonce
    ));
}

#[test]
fn rejects_wrong_old_state_root() {
    let mut input = happy_path_fixture();
    input.expected_old_state_root = StateRoot::new([0xff; 32]);

    assert!(matches!(
        settlement_error(input),
        SettlementError::OldStateRootMismatch
    ));
}

#[test]
fn rejects_duplicate_account() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(PRICE, *USDC.id())]),
            account(ALICE, vec![balance(ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![],
        vec![],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::DuplicateAccount
    ));
}

#[test]
fn rejects_arithmetic_overflow() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(u128::MAX, *USDC.id())]),
            account(BOB, vec![balance(2, *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, u128::MAX, 2, 0, 1),
            sell(BOB, u128::MAX, 2, 0, 2),
        ],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::ArithmeticOverflow
    ));
}

#[test]
fn conserves_every_asset() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(10_000 * USDC.scale(), *USDC.id())]),
            account(BOB, vec![balance(2 * ETH.scale(), *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(BOB, PRICE, ETH.scale(), 0, 2),
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
            account(
                ALICE,
                vec![
                    balance(ETH.scale(), *ETH.id()),
                    balance(10_000 * USDC.scale(), *USDC.id()),
                ],
            ),
            account(TREASURY, vec![]),
        ],
        vec![
            buy(ALICE, PRICE, ETH.scale(), 0, 1),
            sell(ALICE, PRICE, ETH.scale(), 1, 2),
        ],
        vec![0],
        vec![1],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::SelfTrade
    ));
}

#[test]
fn rejects_zero_quantity() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(PRICE, *USDC.id())]),
            account(TREASURY, vec![]),
        ],
        vec![buy(ALICE, PRICE, 0, 0, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::ZeroQuantity
    ));
}

#[test]
fn rejects_zero_price() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(PRICE, *USDC.id())]),
            account(TREASURY, vec![]),
        ],
        vec![buy(ALICE, 0, ETH.scale(), 0, 1)],
        vec![0],
        vec![],
        BUYER_FEE_BPS,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::ZeroPrice
    ));
}

#[test]
fn rejects_quote_amount_that_rounds_to_zero() {
    let input = batch(
        vec![
            account(ALICE, vec![balance(1, *USDC.id())]),
            account(BOB, vec![balance(1, *ETH.id())]),
            account(TREASURY, vec![]),
        ],
        vec![buy(ALICE, 1, 1, 0, 1), sell(BOB, 1, 1, 0, 2)],
        vec![0],
        vec![1],
        0,
    );

    assert!(matches!(
        settlement_error(input),
        SettlementError::TradeValueRoundsToZero
    ));
}
