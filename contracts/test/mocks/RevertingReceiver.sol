// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

/// Always reverts on receiving native ETH — used to test that a single unreceivable
/// payout is credited to `pendingWithdrawals` instead of blocking an entire batch.
contract RevertingReceiver {
    receive() external payable {
        revert("RevertingReceiver: rejects ETH");
    }
}
