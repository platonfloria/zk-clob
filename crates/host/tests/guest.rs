use sp1_sdk::{
    ProveRequest, Prover, ProverClient, ProvingKey, SP1Stdin, include_elf, utils::setup_logger,
};
use zk_clob_core::{PublicOutput, settle_batch};
use zk_clob_test_utils::{happy_path_fixture, multi_market_happy_path_fixture};

const GUEST_ELF: sp1_sdk::Elf = include_elf!("zk-clob-guest");

#[tokio::test]
async fn guest_matches_native_settlement() {
    setup_logger();

    let expected =
        settle_batch(multi_market_happy_path_fixture()).expect("native settlement should succeed");
    let mut stdin = SP1Stdin::new();
    stdin.write(&multi_market_happy_path_fixture());

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

    eprintln!("guest syscalls: {:#?}", report.syscall_counts);
    eprintln!(
        "guest executed in {} cycles",
        report.total_instruction_count()
    );
}

#[tokio::test]
#[ignore = "generates a real SP1 proof"]
async fn proves_and_verifies_guest_settlement() {
    setup_logger();

    let expected = settle_batch(happy_path_fixture()).expect("native settlement should succeed");
    let mut stdin = SP1Stdin::new();
    stdin.write(&happy_path_fixture());

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
