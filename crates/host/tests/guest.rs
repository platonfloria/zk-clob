use alloy_sol_types::SolValue as _;
use sp1_sdk::{ProveRequest, Prover, ProverClient, ProvingKey, SP1Stdin, include_elf, utils::setup_logger};
use zk_clob_core::{PublicOutput, settle_batch};
use zk_clob_test_utils::{happy_path_fixture, multi_market_happy_path_fixture};

const GUEST_ELF: sp1_sdk::Elf = include_elf!("zk-clob-guest");

#[tokio::test]
async fn guest_matches_native_settlement() {
    setup_logger();

    let input = multi_market_happy_path_fixture();
    let mut stdin = SP1Stdin::new();
    stdin.write(&input);
    let expected = settle_batch(input).expect("native settlement should succeed");

    eprintln!("creating mock prover");
    let client = ProverClient::builder().mock().build().await;

    eprintln!("mock prover ready; executing guest");
    let (public_values, report) = client
        .execute(GUEST_ELF, stdin)
        .await
        .expect("guest execution should succeed");
    eprintln!("guest execution complete");

    let actual = PublicOutput::abi_decode(public_values.as_slice()).expect("guest output should be valid ABI");
    assert_eq!(&actual, expected.public());

    eprintln!("guest syscalls: {:#?}", report.syscall_counts);
    eprintln!("guest executed in {} cycles", report.total_instruction_count());
}

#[tokio::test]
#[ignore = "generates a real SP1 proof"]
async fn proves_and_verifies_guest_settlement() {
    setup_logger();

    let input = happy_path_fixture();
    let mut stdin = SP1Stdin::new();
    stdin.write(&input);
    let expected = settle_batch(input).expect("native settlement should succeed");

    eprintln!("creating CPU prover");
    let client = ProverClient::builder().cpu().build().await;

    eprintln!("setting up proving and verification keys");
    let proving_key = client.setup(GUEST_ELF).await.expect("guest setup should succeed");

    eprintln!("generating Groth16 proof");
    let proof = client
        .prove(&proving_key, stdin)
        .groth16()
        .await
        .expect("proof generation should succeed");

    eprintln!("verifying Groth16 proof");
    client
        .verify(&proof, proving_key.verifying_key(), None)
        .expect("proof verification should succeed");

    let actual = PublicOutput::abi_decode(proof.public_values.as_slice()).expect("proof output should be valid ABI");
    assert_eq!(&actual, expected.public());
}
