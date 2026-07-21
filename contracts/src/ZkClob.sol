// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "openzeppelin-contracts/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "openzeppelin-contracts/contracts/utils/ReentrancyGuard.sol";

import {IZkClob} from "./IZkClob.sol";

contract ZkClob is IZkClob, ReentrancyGuard {
    using SafeERC20 for IERC20;

    uint256 private constant PUBLIC_VALUES_LENGTH = 13 * 32;
    bytes private constant DEPOSITS_HASH_DOMAIN = "ZKCLOB_DEPOSITS_V1";
    bytes private constant WITHDRAWALS_HASH_DOMAIN = "ZKCLOB_WITHDRAWALS_V1";

    ISP1Verifier public immutable VERIFIER;
    bytes32 public immutable PROGRAM_VKEY;
    bytes32 public immutable EXCHANGE_ID;
    bytes32 public immutable CONFIG_HASH;
    uint32 public immutable PROTOCOL_VERSION;

    bytes32 public override stateRoot;
    uint64 public override nextBatchId;
    uint64 public override nextDepositId;
    uint64 public override nextUnprocessedDeposit;
    mapping(uint64 depositId => Deposit) public override deposits;

    constructor(
        ISP1Verifier verifier_,
        bytes32 programVKey_,
        bytes32 exchangeId_,
        bytes32 configHash_,
        uint32 protocolVersion_,
        bytes32 initialStateRoot_,
        uint64 initialBatchId_
    ) {
        if (address(verifier_) == address(0)) revert ZeroVerifier();

        VERIFIER = verifier_;
        PROGRAM_VKEY = programVKey_;
        EXCHANGE_ID = exchangeId_;
        CONFIG_HASH = configHash_;
        PROTOCOL_VERSION = protocolVersion_;
        stateRoot = initialStateRoot_;
        nextBatchId = initialBatchId_;
    }

    function deposit() external payable nonReentrant returns (uint64 depositId) {
        depositId = _queueDeposit(msg.sender, address(0), msg.value);
    }

    function deposit(address token, uint256 amount) external payable nonReentrant returns (uint64 depositId) {
        if (token == address(0)) revert InvalidToken(token);
        if (msg.value != 0) revert UnexpectedNativeValue(msg.value);
        _validateDepositAmount(amount);

        IERC20 asset = IERC20(token);
        uint256 balanceBefore = asset.balanceOf(address(this));
        asset.safeTransferFrom(msg.sender, address(this), amount);
        uint256 balanceAfter = asset.balanceOf(address(this));
        if (balanceAfter < balanceBefore) revert TokenTransferAmountMismatch(token, amount, 0);
        uint256 received = balanceAfter - balanceBefore;
        if (received != amount) revert TokenTransferAmountMismatch(token, amount, received);

        depositId = _queueDeposit(msg.sender, token, amount);
    }

    function settle(bytes calldata publicValues, bytes calldata proof, Withdrawal[] calldata withdrawals)
        external
        nonReentrant
    {
        if (publicValues.length != PUBLIC_VALUES_LENGTH) {
            revert InvalidPublicValuesLength(publicValues.length);
        }

        PublicOutput memory output = abi.decode(publicValues, (PublicOutput));
        SigningDomain memory domain = output.domain;

        if (domain.protocolVersion != PROTOCOL_VERSION) {
            revert WrongProtocolVersion(PROTOCOL_VERSION, domain.protocolVersion);
        }
        if (uint256(domain.chainId) != block.chainid) {
            revert WrongChain(block.chainid, domain.chainId);
        }
        if (domain.exchangeId != EXCHANGE_ID) {
            revert WrongExchange(EXCHANGE_ID, domain.exchangeId);
        }
        if (output.configHash != CONFIG_HASH) {
            revert WrongConfig(CONFIG_HASH, output.configHash);
        }
        if (output.batchId != nextBatchId) {
            revert WrongBatchId(nextBatchId, output.batchId);
        }
        if (output.oldStateRoot != stateRoot) {
            revert StaleStateRoot(stateRoot, output.oldStateRoot);
        }
        if (output.oldDepositCursor != nextUnprocessedDeposit) {
            revert WrongDepositCursor(nextUnprocessedDeposit, output.oldDepositCursor);
        }
        if (output.newDepositCursor < output.oldDepositCursor || output.newDepositCursor > nextDepositId) {
            revert InvalidDepositCursorAdvance(output.oldDepositCursor, output.newDepositCursor, nextDepositId);
        }

        bytes32 depositsHash = _hashDeposits(output.oldDepositCursor, output.newDepositCursor);
        if (depositsHash != output.consumedDepositsHash) {
            revert ConsumedDepositsHashMismatch(depositsHash, output.consumedDepositsHash);
        }
        bytes32 withdrawalsHash = _hashWithdrawals(withdrawals);
        if (withdrawalsHash != output.withdrawalsHash) {
            revert WithdrawalsHashMismatch(withdrawalsHash, output.withdrawalsHash);
        }

        VERIFIER.verifyProof(PROGRAM_VKEY, publicValues, proof);

        stateRoot = output.newStateRoot;
        nextUnprocessedDeposit = output.newDepositCursor;
        nextBatchId++;

        _executeWithdrawals(withdrawals);

        emit BatchSettled(output.batchId, output.oldStateRoot, output.newStateRoot, output.batchHash, output.tradesHash);
    }

    function _queueDeposit(address account, address asset, uint256 amount) private returns (uint64 depositId) {
        _validateDepositAmount(amount);

        depositId = nextDepositId;
        nextDepositId = depositId + 1;
        deposits[depositId] = Deposit(account, asset, uint128(amount));

        emit DepositQueued(depositId, account, asset, uint128(amount));
    }

    function _validateDepositAmount(uint256 amount) private pure {
        if (amount == 0) revert ZeroDepositAmount();
        if (amount > type(uint128).max) revert DepositAmountOverflow(amount);
    }

    function _hashDeposits(uint64 start, uint64 end) private view returns (bytes32) {
        bytes memory encoded = abi.encodePacked(DEPOSITS_HASH_DOMAIN, uint64(end - start));
        for (uint64 id = start; id < end; id++) {
            Deposit storage queued = deposits[id];
            encoded = bytes.concat(
                encoded, abi.encodePacked(id, queued.account, bytes32(uint256(uint160(queued.asset))), queued.amount)
            );
        }
        return sha256(encoded);
    }

    function _hashWithdrawals(Withdrawal[] calldata withdrawals) private pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked(WITHDRAWALS_HASH_DOMAIN, uint64(withdrawals.length));
        for (uint256 index; index < withdrawals.length; index++) {
            Withdrawal calldata withdrawal = withdrawals[index];
            encoded = bytes.concat(
                encoded,
                abi.encodePacked(
                    withdrawal.account, withdrawal.recipient, withdrawal.asset, withdrawal.amount, withdrawal.nonce
                )
            );
        }
        return sha256(encoded);
    }

    function _executeWithdrawals(Withdrawal[] calldata withdrawals) private {
        for (uint256 index; index < withdrawals.length; index++) {
            Withdrawal calldata withdrawal = withdrawals[index];
            if (uint256(withdrawal.asset) >> 160 != 0) revert InvalidWithdrawalAsset(withdrawal.asset);

            address asset = address(uint160(uint256(withdrawal.asset)));
            if (asset == address(0)) {
                (bool success,) = withdrawal.recipient.call{value: withdrawal.amount}("");
                if (!success) revert NativeWithdrawalFailed(withdrawal.recipient, withdrawal.amount);
            } else {
                IERC20(asset).safeTransfer(withdrawal.recipient, withdrawal.amount);
            }

            emit WithdrawalExecuted(
                withdrawal.account, withdrawal.recipient, withdrawal.asset, withdrawal.amount, withdrawal.nonce
            );
        }
    }
}
