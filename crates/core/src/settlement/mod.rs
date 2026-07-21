mod assets;
mod deposits;
mod errors;
mod matching;
mod nonces;
mod output;
mod validation;
mod withdrawals;

use crate::{BatchInput, BatchOutput, hashing::DomainSha256Hash as _};

use self::{
    assets::AssetTracker,
    deposits::apply_deposits,
    matching::match_and_settle,
    nonces::consume_nonces,
    output::build_output,
    validation::{
        build_validated_books, validate_accounts, validate_config, validate_deposits, validate_limits, validate_nonces,
        validate_orders, validate_withdrawals,
    },
    withdrawals::apply_withdrawals,
};

pub use errors::SettlementError;

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub fn settle_batch(input: BatchInput) -> Result<BatchOutput, SettlementError> {
    cycle_tracker![
        "batch-validation",
        validate_limits(&input)?;
        let (
            metadata,
            expected_old_state_root,
            mut state,
            old_deposit_cursor,
            deposits,
            orders,
            withdrawals,
            order_books,
            config,
        ) = (
            input.metadata,
            input.expected_old_state_root,
            input.state,
            input.old_deposit_cursor,
            input.deposits,
            input.orders,
            input.withdrawals,
            input.order_books,
            input.config,
        );

        validate_config(&config, state.accounts())?;
        validate_accounts(state.accounts())?;
        let new_deposit_cursor = validate_deposits(&deposits, old_deposit_cursor, &config)?;
    ];

    cycle_tracker![
        "input-commitments",
        let old_state_root = state.root()?;
        if old_state_root != expected_old_state_root {
            return Err(SettlementError::OldStateRootMismatch);
        }
        let config_hash = config.hash();
        let batch_hash = (
            &metadata,
            &old_state_root,
            &config_hash,
            orders.as_slice(),
        )
            .hash();
    ];

    cycle_tracker![
        "conservation-baseline",
        let mut asset_tracker = AssetTracker::default();
        asset_tracker.add_accounts(state.accounts())?;
        asset_tracker.add_deposits(&deposits)?;
        asset_tracker.subtract_withdrawals(&withdrawals)?;
    ];

    apply_deposits(state.accounts_mut(), &deposits)?;

    cycle_tracker![
        "operation-validation",
        validate_withdrawals(&withdrawals, state.accounts(), &config)?;
        validate_orders(&orders, state.accounts(), &config)?;
        validate_nonces(&orders, &withdrawals, state.accounts())?;
    ];

    apply_withdrawals(state.accounts_mut(), &withdrawals)?;
    consume_nonces(state.accounts_mut(), &orders, &withdrawals)?;
    let books = build_validated_books(&orders, &order_books, &config)?;
    let trades = match_and_settle(state.accounts_mut(), books, &config)?;

    cycle_tracker![
        "conservation-check",
        asset_tracker.subtract_accounts(state.accounts())?;
        if !asset_tracker.is_empty() {
            return Err(SettlementError::AssetConservationViolation);
        }
    ];

    cycle_tracker![
        "new-state-commitment",
        let new_state_root = state.root()?;
    ];

    Ok(build_output(
        metadata,
        config_hash,
        batch_hash,
        old_state_root,
        new_state_root,
        state.into_accounts(),
        trades,
        old_deposit_cursor,
        new_deposit_cursor,
        &deposits,
        &withdrawals,
    ))
}
