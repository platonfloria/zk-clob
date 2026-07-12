use sp1_sdk::{
    ProveRequest, Prover, ProverClient, ProvingKey, SP1Stdin, include_elf, utils::setup_logger,
};
use zk_clob_core::{
    Account, AccountId, AssetBalance, AssetConfig, AssetId, BatchInput, ExchangeConfig, ExchangeId,
    FeeConfig, MarketConfig, MarketId, Order, PublicOutput, Side, compute_state_root, settle_batch,
};

const GUEST_ELF: sp1_sdk::Elf = include_elf!("zk-clob-guest");

const ETH: AssetConfig = AssetConfig::new(AssetId::new([1; 32]), 10u128.pow(18));
const USDC: AssetConfig = AssetConfig::new(AssetId::new([2; 32]), 10u128.pow(6));
const ETH_USDC: MarketId = MarketId::new([3; 32]);
const ALICE: AccountId = AccountId::new([1; 20]);
const BOB: AccountId = AccountId::new([2; 20]);
const TREASURY: AccountId = AccountId::new([3; 20]);
const EXCHANGE: ExchangeId = ExchangeId::new([4; 32]);

fn batch_input() -> BatchInput {
    let accounts = vec![
        Account::new(
            ALICE,
            vec![AssetBalance::new(USDC.id(), 10_000 * USDC.scale())],
            0,
        ),
        Account::new(BOB, vec![AssetBalance::new(ETH.id(), ETH.scale())], 0),
        Account::new(TREASURY, vec![], 0),
    ];
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
        vec![MarketConfig::new(ETH_USDC, ETH.id(), USDC.id())],
        FeeConfig::new(TREASURY, 10),
    );
    let old_state_root = compute_state_root(&accounts);

    BatchInput::new(
        1,
        31_337,
        EXCHANGE,
        0,
        old_state_root,
        accounts,
        orders,
        config,
    )
}

#[tokio::test]
async fn guest_matches_native_settlement() {
    setup_logger();

    let expected = settle_batch(batch_input()).expect("native settlement should succeed");
    let mut stdin = SP1Stdin::new();
    stdin.write(&batch_input());

    eprintln!("creating mock prover");
    let client = ProverClient::builder().mock().build().await;

    eprintln!("mock prover ready; executing guest");
    let (mut public_values, report) = client
        .execute(GUEST_ELF, stdin)
        .await
        .expect("guest execution should succeed");
    eprintln!("guest execution complete");

    let actual = public_values.read::<PublicOutput>();
    assert_eq!(actual.old_state_root(), expected.public().old_state_root());
    assert_eq!(actual.new_state_root(), expected.public().new_state_root());
    assert_eq!(actual.config_hash(), expected.public().config_hash());
    assert_eq!(actual.batch_hash(), expected.public().batch_hash());
    assert_eq!(actual.trades_hash(), expected.public().trades_hash());

    eprintln!(
        "guest executed in {} cycles",
        report.total_instruction_count()
    );
}

#[tokio::test]
#[ignore = "generates a real SP1 proof"]
async fn proves_and_verifies_guest_settlement() {
    setup_logger();

    let expected = settle_batch(batch_input()).expect("native settlement should succeed");
    let mut stdin = SP1Stdin::new();
    stdin.write(&batch_input());

    eprintln!("creating CPU prover");
    let client = ProverClient::builder().cpu().build().await;

    eprintln!("setting up proving and verification keys");
    let proving_key = client
        .setup(GUEST_ELF)
        .await
        .expect("guest setup should succeed");

    eprintln!("generating compressed proof");
    let mut proof = client
        .prove(&proving_key, stdin)
        .core()
        .await
        .expect("proof generation should succeed");

    eprintln!("verifying compressed proof");
    client
        .verify(&proof, proving_key.verifying_key(), None)
        .expect("proof verification should succeed");

    let actual = proof.public_values.read::<PublicOutput>();
    assert_eq!(actual.old_state_root(), expected.public().old_state_root());
    assert_eq!(actual.new_state_root(), expected.public().new_state_root());
    assert_eq!(actual.config_hash(), expected.public().config_hash());
    assert_eq!(actual.batch_hash(), expected.public().batch_hash());
    assert_eq!(actual.trades_hash(), expected.public().trades_hash());
}
