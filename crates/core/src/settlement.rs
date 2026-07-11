use crate::{BatchInput, BatchOutput, SettlementError};

pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    let _ = input;
    todo!("batch settlement is not implemented yet")
}
