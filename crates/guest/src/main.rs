#![no_main]

sp1_zkvm::entrypoint!(main);

use alloy_sol_types::SolValue as _;
use zk_clob_core::{BatchInput, settle_batch};

pub fn main() {
    let input = sp1_zkvm::io::read::<BatchInput>();

    let output = settle_batch(input).expect("batch settlement failed");

    sp1_zkvm::io::commit_slice(&output.public().abi_encode());
}
