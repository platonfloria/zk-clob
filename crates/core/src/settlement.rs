use crate::{
    BatchInput, BatchOutput, SettlementError,
    hashing::compute_state_root,
    validation::{validate_accounts, validate_config, validate_limits, validate_orders},
};

pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    validate_limits(&input)?;
    validate_config(input.config(), input.accounts())?;
    validate_accounts(input.accounts())?;
    validate_orders(input.orders(), input.accounts(), input.config())?;

    let computed_old_root = compute_state_root(input.accounts());
    if &computed_old_root != input.expected_old_state_root() {
        return Err(SettlementError::OldStateRootMismatch);
    }

    todo!("batch settlement is not implemented yet")
}
