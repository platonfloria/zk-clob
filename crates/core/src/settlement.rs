use crate::{BatchInput, BatchOutput, SettlementError, hashing::compute_state_root};

pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    let computed_old_root = compute_state_root(input.accounts());

    if &computed_old_root != input.expected_old_state_root() {
        return Err(SettlementError::OldStateRootMismatch);
    }
    todo!("batch settlement is not implemented yet")
}
