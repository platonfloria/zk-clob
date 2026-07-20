use std::{fs, path::PathBuf, time::Instant};

use alloy_sol_types::SolValue as _;
use clap::{Parser, Subcommand, ValueEnum};
use color_eyre::eyre::{Context, Result, eyre};
use sp1_sdk::{
    HashableKey, ProveRequest, Prover, ProverClient, ProvingKey, SP1Stdin, include_elf,
    utils::setup_logger,
};
use zk_clob_core::{BatchInput, PublicOutput};
use zk_clob_test_utils::{happy_path_fixture, multi_market_happy_path_fixture};

const GUEST_ELF: sp1_sdk::Elf = include_elf!("zk-clob-guest");

#[derive(Parser)]
#[command(about = "Execute and prove zk-clob batch settlements")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute the guest without generating a proof.
    Execute {
        #[arg(long, value_enum, default_value_t = Fixture::MultiMarket)]
        fixture: Fixture,
    },
    /// Generate and locally verify an on-chain Groth16 proof.
    Prove {
        #[arg(long, value_enum, default_value_t = Fixture::MultiMarket)]
        fixture: Fixture,
        #[arg(long, default_value = "artifacts")]
        output_dir: PathBuf,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Fixture {
    HappyPath,
    MultiMarket,
}

impl Fixture {
    fn build(self) -> BatchInput {
        match self {
            Self::HappyPath => happy_path_fixture(),
            Self::MultiMarket => multi_market_happy_path_fixture(),
        }
    }
}

fn stdin(input: &BatchInput) -> SP1Stdin {
    let mut stdin = SP1Stdin::new();
    stdin.write(input);
    stdin
}

fn print_public_output(output: &PublicOutput) {
    println!("old state root:  {:?}", output.oldStateRoot);
    println!("new state root:  {:?}", output.newStateRoot);
    println!("config hash:     {:?}", output.configHash);
    println!("batch hash:      {:?}", output.batchHash);
    println!("trades hash:     {:?}", output.tradesHash);
    println!(
        "deposit cursor:  {} -> {}",
        output.oldDepositCursor, output.newDepositCursor
    );
    println!("deposits hash:   {:?}", output.consumedDepositsHash);
}

async fn execute(input: BatchInput) -> Result<()> {
    let client = ProverClient::builder().light().build().await;
    let started = Instant::now();
    let (public_values, report) = client
        .execute(GUEST_ELF, stdin(&input))
        .await
        .context("guest execution failed")?;
    let elapsed = started.elapsed();
    let output = PublicOutput::abi_decode(public_values.as_slice())?;

    print_public_output(&output);
    println!("cycles:           {}", report.total_instruction_count());
    println!("execution time:   {elapsed:?}");
    Ok(())
}

async fn prove(input: BatchInput, output_dir: PathBuf) -> Result<()> {
    fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "failed to create artifact directory {}",
            output_dir.display()
        )
    })?;

    let initialization_started = Instant::now();
    let client = ProverClient::from_env().await;
    println!(
        "prover initialization: {:?}",
        initialization_started.elapsed()
    );

    let setup_started = Instant::now();
    let proving_key = client
        .setup(GUEST_ELF)
        .await
        .map_err(|error| eyre!("guest setup failed: {error:#}"))?;
    println!("program setup:         {:?}", setup_started.elapsed());

    let proving_started = Instant::now();
    let proof = client
        .prove(&proving_key, stdin(&input))
        .groth16()
        .await
        .map_err(|error| eyre!("Groth16 proof generation failed: {error:#}"))?;
    println!("Groth16 proving:       {:?}", proving_started.elapsed());

    let verification_started = Instant::now();
    client
        .verify(&proof, proving_key.verifying_key(), None)
        .context("local Groth16 proof verification failed")?;
    println!(
        "local verification:    {:?}",
        verification_started.elapsed()
    );

    let proof_bundle_path = output_dir.join("proof-with-public-values.bin");
    proof
        .save(&proof_bundle_path)
        .map_err(|error| eyre!("failed to save {}: {error:#}", proof_bundle_path.display()))?;

    let proof_bytes_path = output_dir.join("proof.bin");
    fs::write(&proof_bytes_path, proof.bytes())
        .with_context(|| format!("failed to save {}", proof_bytes_path.display()))?;

    let public_values_path = output_dir.join("public-values.bin");
    fs::write(&public_values_path, proof.public_values.as_slice())
        .with_context(|| format!("failed to save {}", public_values_path.display()))?;

    let vkey_path = output_dir.join("program-vkey.txt");
    fs::write(&vkey_path, proving_key.verifying_key().bytes32())
        .with_context(|| format!("failed to save {}", vkey_path.display()))?;

    // TODO(solidity): encode public values and package proof calldata according
    // to the zk-clob Solidity contract ABI once that contract is implemented.

    let output = PublicOutput::abi_decode(proof.public_values.as_slice())?;
    print_public_output(&output);
    println!("artifacts:             {}", output_dir.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    setup_logger();

    match Cli::parse().command {
        Command::Execute { fixture } => execute(fixture.build()).await,
        Command::Prove {
            fixture,
            output_dir,
        } => prove(fixture.build(), output_dir).await,
    }
}
