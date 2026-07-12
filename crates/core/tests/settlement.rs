use zk_clob_core::{AccountId, BatchHash, ConfigHash, StateRoot, compute_state_root, settle_batch};
use zk_clob_test_utils::{
    ALICE, BOB, ETH, TREASURY, USDC, happy_path_fixture, multi_market_happy_path_fixture,
};

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
    assert_eq!(public.old_state_root(), &expected_old_state_root);
    assert_eq!(
        public.new_state_root(),
        &compute_state_root(output.updated_accounts())
    );

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
fn settles_twenty_orders_across_two_markets() {
    let output = settle_batch(multi_market_happy_path_fixture())
        .expect("multi-market settlement should succeed");

    assert_eq!(output.trades().len(), 18);
}
