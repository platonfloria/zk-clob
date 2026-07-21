use std::collections::BTreeMap;

use alloy_primitives::B256;
use proptest::prelude::*;
use zk_clob_core::{
    AccountId, AssetBalance, AssetConfig, AssetId, BatchInput, BatchOutput, ExchangeConfig, ExchangeId, FeeConfig,
    MarketConfig, MarketId, MarketOrderBook, Side, State, settle_batch,
};
use zk_clob_test_utils::{ALICE, BOB, CAROL, DAVE, TREASURY, TestSigner};

const BASE: AssetConfig = AssetConfig::new(AssetId::new(B256::new([1; 32])), 1);
const QUOTE: AssetConfig = AssetConfig::new(AssetId::new(B256::new([2; 32])), 1);
const MARKET: MarketId = MarketId::new(B256::new([1; 32]));
const EXCHANGE: ExchangeId = ExchangeId::new([1; 32]);
const BUYER_QUOTE_BALANCE: u128 = 1_000_000;

#[derive(Clone, Debug)]
struct SellOrder {
    price: u128,
    quantity: u128,
}

#[derive(Clone, Debug)]
struct SettlementCase {
    buy_price: u128,
    buy_quantity: u128,
    sells: Vec<SellOrder>,
    buyer_is_older: bool,
    buyer_fee_bps: u16,
}

fn settlement_case() -> impl Strategy<Value = SettlementCase> {
    (
        1u128..101,
        1u128..51,
        prop::collection::vec((1u128..101, 1u128..31), 1..4),
        any::<bool>(),
        0u16..101,
    )
        .prop_map(
            |(buy_price, buy_quantity, sells, buyer_is_older, buyer_fee_bps)| SettlementCase {
                buy_price,
                buy_quantity,
                sells: sells
                    .into_iter()
                    .map(|(price, quantity)| SellOrder { price, quantity })
                    .collect(),
                buyer_is_older,
                buyer_fee_bps,
            },
        )
}

fn seller(index: usize) -> &'static TestSigner {
    [&BOB, &CAROL, &DAVE][index]
}

fn buy_sequence(case: &SettlementCase) -> u64 {
    if case.buyer_is_older {
        0
    } else {
        case.sells.len() as u64
    }
}

fn sell_sequence(case: &SettlementCase, index: usize) -> u64 {
    if case.buyer_is_older {
        index as u64 + 1
    } else {
        index as u64
    }
}

fn build_input(case: &SettlementCase, account_rotation: usize) -> BatchInput {
    let mut accounts = vec![ALICE.account(vec![AssetBalance::new(*QUOTE.id(), BUYER_QUOTE_BALANCE)])];
    accounts.extend(
        case.sells
            .iter()
            .enumerate()
            .map(|(index, sell)| seller(index).account(vec![AssetBalance::new(*BASE.id(), sell.quantity)])),
    );
    accounts.push(TREASURY.account(vec![]));

    // The host may receive accounts in any order, but the guest's state encoding
    // requires the witness to be canonical before settlement.
    let account_count = accounts.len();
    accounts.rotate_left(account_rotation % account_count);
    accounts.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    let mut orders = vec![ALICE.order(
        MARKET,
        Side::Buy,
        case.buy_price,
        case.buy_quantity,
        0,
        buy_sequence(case),
    )];
    orders.extend(case.sells.iter().enumerate().map(|(index, sell)| {
        seller(index).order(
            MARKET,
            Side::Sell,
            sell.price,
            sell.quantity,
            0,
            sell_sequence(case, index),
        )
    }));

    let mut sell_indices: Vec<u32> = (1..orders.len())
        .map(|index| u32::try_from(index).expect("small generated order book"))
        .collect();
    sell_indices.sort_unstable_by(|left, right| {
        let left = &orders[*left as usize];
        let right = &orders[*right as usize];
        left.price()
            .cmp(&right.price())
            .then_with(|| left.sequence().cmp(&right.sequence()))
    });

    let state = State::new(accounts);
    let old_state_root = state.root();
    let state = state.witness().expect("full-state witness should be valid");
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
        vec![MarketOrderBook::new(MARKET, vec![0], sell_indices)],
        ExchangeConfig::new(
            vec![BASE, QUOTE],
            vec![MarketConfig::new(MARKET, *BASE.id(), *QUOTE.id())],
            FeeConfig::new(TREASURY.id(), case.buyer_fee_bps),
        ),
    )
}

fn settle_case(case: &SettlementCase, account_rotation: usize) -> BatchOutput {
    settle_batch(build_input(case, account_rotation)).expect("generated valid batch should settle")
}

fn total_balance(output: &BatchOutput, asset: &AssetId) -> u128 {
    output
        .updated_accounts()
        .iter()
        .map(|account| account.balance(asset))
        .sum()
}

fn seller_fills(output: &BatchOutput) -> BTreeMap<AccountId, u128> {
    let mut fills = BTreeMap::new();
    for trade in output.trades() {
        *fills.entry(*trade.seller()).or_insert(0) += trade.quantity();
    }
    fills
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 64,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn settlement_conserves_each_asset(case in settlement_case()) {
        let initial_base: u128 = case.sells.iter().map(|sell| sell.quantity).sum();
        let output = settle_case(&case, 0);

        prop_assert_eq!(total_balance(&output, BASE.id()), initial_base);
        prop_assert_eq!(total_balance(&output, QUOTE.id()), BUYER_QUOTE_BALANCE);
    }

    #[test]
    fn remaining_quantity_never_exceeds_original(case in settlement_case()) {
        let output = settle_case(&case, 0);
        let buy_fill: u128 = output.trades().iter().map(|trade| trade.quantity()).sum();
        let fills = seller_fills(&output);

        prop_assert!(buy_fill <= case.buy_quantity);
        for (index, sell) in case.sells.iter().enumerate() {
            prop_assert!(fills.get(&seller(index).id()).copied().unwrap_or(0) <= sell.quantity);
        }
    }

    #[test]
    fn every_trade_quantity_is_positive(case in settlement_case()) {
        let output = settle_case(&case, 0);

        prop_assert!(output.trades().iter().all(|trade| trade.quantity() > 0));
    }

    #[test]
    fn trade_price_comes_from_the_older_order(case in settlement_case()) {
        let output = settle_case(&case, 0);
        let buyer_sequence = buy_sequence(&case);

        for trade in output.trades() {
            let seller_index = (0..case.sells.len())
                .find(|&index| trade.seller() == &seller(index).id())
                .expect("trade seller must come from the generated book");
            let expected_price = if buyer_sequence < sell_sequence(&case, seller_index) {
                case.buy_price
            } else {
                case.sells[seller_index].price
            };

            prop_assert!(trade.market_id() == &MARKET);
            prop_assert_eq!(trade.buyer(), &ALICE.id());
            prop_assert_eq!(trade.price(), expected_price);
        }
    }

    #[test]
    fn no_crossing_pair_remains_after_matching(case in settlement_case()) {
        let output = settle_case(&case, 0);
        let buy_fill: u128 = output.trades().iter().map(|trade| trade.quantity()).sum();
        let fills = seller_fills(&output);
        let buy_remaining = case.buy_quantity - buy_fill;

        if buy_remaining > 0 {
            for (index, sell) in case.sells.iter().enumerate() {
                let sell_remaining = sell.quantity
                    - fills.get(&seller(index).id()).copied().unwrap_or(0);
                prop_assert!(sell_remaining == 0 || case.buy_price < sell.price);
            }
        }
    }

    #[test]
    fn canonical_settlement_is_deterministic(case in settlement_case()) {
        let first = settle_case(&case, 0);
        let second = settle_case(&case, 0);

        prop_assert_eq!(first.public(), second.public());
    }
}
