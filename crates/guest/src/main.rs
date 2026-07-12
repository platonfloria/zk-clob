#![no_main]

sp1_zkvm::entrypoint!(main);

use zk_clob_core::{BatchInput, settle_batch};

pub fn main() {
    let input = sp1_zkvm::io::read::<BatchInput>();

    let output = settle_batch(input).expect("batch settlement failed");

    sp1_zkvm::io::commit(output.public());
}
