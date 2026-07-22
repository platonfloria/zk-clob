// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";
import {IERC20} from "openzeppelin-contracts/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "openzeppelin-contracts/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "openzeppelin-contracts/contracts/utils/ReentrancyGuard.sol";

import {IZkClob} from "./IZkClob.sol";

contract ZkClob is IZkClob, ReentrancyGuard {
    using SafeERC20 for IERC20;

    uint256 private constant PUBLIC_VALUES_LENGTH = 17 * 32;
    bytes private constant DEPOSITS_HASH_DOMAIN = "ZKCLOB_DEPOSITS_V1";
    bytes private constant WITHDRAWALS_HASH_DOMAIN = "ZKCLOB_WITHDRAWALS_V1";
    bytes private constant FORCED_WITHDRAWALS_HASH_DOMAIN = "ZKCLOB_FORCED_WITHDRAWALS_V1";

    ISP1Verifier public immutable VERIFIER;
    bytes32 public immutable PROGRAM_VKEY;
    address public immutable EXCHANGE_ID;
    bytes32 public immutable CONFIG_HASH;
    uint32 public immutable PROTOCOL_VERSION;
    uint64 public immutable FORCED_WITHDRAWAL_DELAY;

    bytes32 public override stateRoot;
    uint64 public override nextBatchId;
    uint64 public override nextDepositId;
    uint64 public override nextUnprocessedDeposit;
    mapping(uint64 depositId => Deposit) public override deposits;
    uint64 public override nextForcedWithdrawalId;
    uint64 public override nextUnprocessedForcedWithdrawal;
    mapping(uint64 requestId => ForcedWithdrawalRequest) public override forcedWithdrawalRequests;
    bool public override escapeMode;

    constructor(
        ISP1Verifier verifier_,
        bytes32 programVKey_,
        bytes32 configHash_,
        uint32 protocolVersion_,
        bytes32 initialStateRoot_,
        uint64 initialBatchId_,
        uint64 forcedWithdrawalDelay_
    ) {
        if (address(verifier_) == address(0)) revert ZeroVerifier();

        VERIFIER = verifier_;
        PROGRAM_VKEY = programVKey_;
        EXCHANGE_ID = address(this);
        CONFIG_HASH = configHash_;
        PROTOCOL_VERSION = protocolVersion_;
        stateRoot = initialStateRoot_;
        nextBatchId = initialBatchId_;
        FORCED_WITHDRAWAL_DELAY = forcedWithdrawalDelay_;
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

    function requestForcedWithdrawal(address asset, uint128 amount) external returns (uint64 requestId) {
        if (amount == 0) revert ZeroForcedWithdrawalAmount();

        requestId = nextForcedWithdrawalId;
        nextForcedWithdrawalId = requestId + 1;
        uint64 deadline = uint64(block.timestamp) + FORCED_WITHDRAWAL_DELAY;
        forcedWithdrawalRequests[requestId] = ForcedWithdrawalRequest(msg.sender, asset, amount, deadline);

        emit ForcedWithdrawalRequested(requestId, msg.sender, asset, amount, deadline);
    }

    function activateEscapeMode() external {
        if (nextUnprocessedForcedWithdrawal >= nextForcedWithdrawalId) revert NoPendingForcedWithdrawal();

        ForcedWithdrawalRequest storage request = forcedWithdrawalRequests[nextUnprocessedForcedWithdrawal];
        if (block.timestamp < request.deadline) revert ForcedWithdrawalDeadlineNotElapsed(request.deadline);

        escapeMode = true;
        emit EscapeModeActivated(nextUnprocessedForcedWithdrawal, request.deadline);
    }

    function settle(
        bytes calldata publicValues,
        bytes calldata proof,
        Withdrawal[] calldata withdrawals,
        ForcedWithdrawal[] calldata forcedWithdrawals
    ) external nonReentrant {
        if (escapeMode) revert EscapeModeActive();

        PublicOutput memory output = _decodeAndValidatePublicOutput(publicValues);

        _validateConsumedInputs(output);
        _validateWithdrawals(output, withdrawals, forcedWithdrawals);

        VERIFIER.verifyProof(PROGRAM_VKEY, publicValues, proof);

        _applySettlement(output, withdrawals, forcedWithdrawals);
    }

    function _decodeAndValidatePublicOutput(
        bytes calldata publicValues
    ) internal view returns (PublicOutput memory output) {
        if (publicValues.length != PUBLIC_VALUES_LENGTH) {
            revert InvalidPublicValuesLength(publicValues.length);
        }

        output = abi.decode(publicValues, (PublicOutput));
        SigningDomain memory domain = output.domain;

        if (domain.protocolVersion != PROTOCOL_VERSION) {
            revert WrongProtocolVersion(
                PROTOCOL_VERSION,
                domain.protocolVersion
            );
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
            revert WrongDepositCursor(
                nextUnprocessedDeposit,
                output.oldDepositCursor
            );
        }
        if (output.newDepositCursor < output.oldDepositCursor
            || output.newDepositCursor > nextDepositId
        ) {
            revert InvalidDepositCursorAdvance(
                output.oldDepositCursor,
                output.newDepositCursor,
                nextDepositId
            );
        }
        if (output.oldForcedWithdrawalCursor != nextUnprocessedForcedWithdrawal) {
            revert WrongForcedWithdrawalCursor(
                nextUnprocessedForcedWithdrawal,
                output.oldForcedWithdrawalCursor
            );
        }
        if (output.newForcedWithdrawalCursor < output.oldForcedWithdrawalCursor
            || output.newForcedWithdrawalCursor > nextForcedWithdrawalId
        ) {
            revert InvalidForcedWithdrawalCursorAdvance(
                output.oldForcedWithdrawalCursor,
                output.newForcedWithdrawalCursor,
                nextForcedWithdrawalId
            );
        }
    }

    function _validateConsumedInputs(
        PublicOutput memory output
    ) internal view {
        bytes32 actualDepositsHash = _hashDeposits(
            output.oldDepositCursor,
            output.newDepositCursor
        );

        if (actualDepositsHash != output.consumedDepositsHash) {
            revert ConsumedDepositsHashMismatch(
                actualDepositsHash,
                output.consumedDepositsHash
            );
        }

        bytes32 actualForcedRequestsHash = _hashForcedWithdrawalRequests(
            output.oldForcedWithdrawalCursor,
            output.newForcedWithdrawalCursor
        );

        if (actualForcedRequestsHash != output.consumedForcedWithdrawalsHash) {
            revert ConsumedForcedWithdrawalsHashMismatch(
                actualForcedRequestsHash,
                output.consumedForcedWithdrawalsHash
            );
        }
    }

    function _validateWithdrawals(
        PublicOutput memory output,
        Withdrawal[] calldata withdrawals,
        ForcedWithdrawal[] calldata forcedWithdrawals
    ) internal pure {
        bytes32 actualWithdrawalsHash = _hashWithdrawals(withdrawals);
        if (actualWithdrawalsHash != output.withdrawalsHash) {
            revert WithdrawalsHashMismatch(
                actualWithdrawalsHash,
                output.withdrawalsHash
            );
        }

        bytes32 actualForcedWithdrawalsHash = _hashForcedWithdrawals(forcedWithdrawals);
        if (actualForcedWithdrawalsHash != output.forcedWithdrawalsHash) {
            revert ForcedWithdrawalsHashMismatch(
                actualForcedWithdrawalsHash,
                output.forcedWithdrawalsHash
            );
        }
    }

    function _applySettlement(
        PublicOutput memory output,
        Withdrawal[] calldata withdrawals,
        ForcedWithdrawal[] calldata forcedWithdrawals
    ) internal {
        stateRoot = output.newStateRoot;
        nextUnprocessedDeposit = output.newDepositCursor;
        nextUnprocessedForcedWithdrawal =
            output.newForcedWithdrawalCursor;

        unchecked {
            ++nextBatchId;
        }

        _executeWithdrawals(withdrawals);
        _executeForcedWithdrawals(forcedWithdrawals);

        emit BatchSettled(
            output.batchId,
            output.oldStateRoot,
            output.newStateRoot,
            output.batchHash,
            output.tradesHash
        );
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

    function _hashForcedWithdrawalRequests(uint64 start, uint64 end) private view returns (bytes32) {
        bytes memory encoded = abi.encodePacked(FORCED_WITHDRAWALS_HASH_DOMAIN, uint64(end - start));
        for (uint64 id = start; id < end; id++) {
            ForcedWithdrawalRequest storage queued = forcedWithdrawalRequests[id];
            encoded = bytes.concat(
                encoded,
                abi.encodePacked(
                    id, queued.account, bytes32(uint256(uint160(queued.asset))), queued.amount
                )
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

    function _hashForcedWithdrawals(ForcedWithdrawal[] calldata forcedWithdrawals) private pure returns (bytes32) {
        bytes memory encoded = abi.encodePacked(FORCED_WITHDRAWALS_HASH_DOMAIN, uint64(forcedWithdrawals.length));
        for (uint256 index; index < forcedWithdrawals.length; index++) {
            ForcedWithdrawal calldata forcedWithdrawal = forcedWithdrawals[index];
            encoded = bytes.concat(
                encoded,
                abi.encodePacked(
                    forcedWithdrawal.id, forcedWithdrawal.account, forcedWithdrawal.asset, forcedWithdrawal.amount
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

    function _executeForcedWithdrawals(ForcedWithdrawal[] calldata forcedWithdrawals) private {
        for (uint256 index; index < forcedWithdrawals.length; index++) {
            ForcedWithdrawal calldata forcedWithdrawal = forcedWithdrawals[index];
            if (forcedWithdrawal.amount == 0) continue;
            if (uint256(forcedWithdrawal.asset) >> 160 != 0) revert InvalidForcedWithdrawalAsset(forcedWithdrawal.asset);

            address asset = address(uint160(uint256(forcedWithdrawal.asset)));
            if (asset == address(0)) {
                (bool success,) = forcedWithdrawal.account.call{value: forcedWithdrawal.amount}("");
                if (!success) revert NativeForcedWithdrawalFailed(forcedWithdrawal.account, forcedWithdrawal.amount);
            } else {
                IERC20(asset).safeTransfer(forcedWithdrawal.account, forcedWithdrawal.amount);
            }

            emit ForcedWithdrawalExecuted(
                forcedWithdrawal.account, forcedWithdrawal.asset, forcedWithdrawal.amount, forcedWithdrawal.id
            );
        }
    }
}
