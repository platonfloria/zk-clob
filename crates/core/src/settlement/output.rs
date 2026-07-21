use crate::{
    Account, BatchHash, BatchMetadata, BatchOutput, ConfigHash, Deposit, ExecutedWithdrawal, PublicOutput,
    SignedWithdrawal, StateRoot, Trade, hashing::DomainSha256Hash as _,
};

#[cfg_attr(feature = "sp1-cycle-tracking", sp1_derive::cycle_tracker)]
pub(super) fn build_output(
    metadata: BatchMetadata,
    config_hash: ConfigHash,
    batch_hash: BatchHash,
    old_state_root: StateRoot,
    new_state_root: StateRoot,
    accounts: Vec<Account>,
    trades: Vec<Trade>,
    old_deposit_cursor: u64,
    new_deposit_cursor: u64,
    deposits: &[Deposit],
    withdrawals: &[SignedWithdrawal],
) -> BatchOutput {
    let executed_withdrawals: Vec<_> = withdrawals.iter().map(ExecutedWithdrawal::from).collect();
    cycle_tracker!["output-construction", {
        BatchOutput::new(
            PublicOutput::new(
                metadata,
                old_state_root,
                new_state_root,
                config_hash,
                batch_hash,
                trades.hash(),
                old_deposit_cursor,
                new_deposit_cursor,
                deposits.hash(),
                executed_withdrawals.as_slice().hash(),
            ),
            accounts,
            trades,
            executed_withdrawals,
        )
    }]
}
