// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

import {PatriciaProof} from "./PatriciaProof.sol";

interface IZkClob {
    struct Deposit {
        address account;
        address asset;
        uint128 amount;
    }

    struct SigningDomain {
        uint32 protocolVersion;
        uint64 chainId;
        address exchangeId;
    }

    struct Withdrawal {
        address account;
        address recipient;
        address asset;
        uint128 amount;
        uint64 nonce;
    }

    struct ForcedWithdrawalRequest {
        address account;
        address asset;
        uint128 amount;
        uint64 deadline;
    }

    struct ForcedWithdrawal {
        uint64 id;
        address account;
        address asset;
        uint128 amount;
    }

    struct PublicOutput {
        SigningDomain domain;
        uint64 batchId;
        bytes32 oldStateRoot;
        bytes32 newStateRoot;
        bytes32 configHash;
        bytes32 batchHash;
        bytes32 tradesHash;
        uint64 oldDepositCursor;
        uint64 newDepositCursor;
        bytes32 consumedDepositsHash;
        uint64 oldForcedWithdrawalCursor;
        bytes32 consumedForcedWithdrawalsHash;
        bytes32 withdrawalsHash;
        bytes32 forcedWithdrawalsHash;
    }

    error InvalidPublicValuesLength(uint256 actual);
    error DepositAmountOverflow(uint256 amount);
    error InvalidToken(address token);
    error TokenTransferAmountMismatch(address token, uint256 expected, uint256 actual);
    error UnexpectedNativeValue(uint256 amount);
    error WrongProtocolVersion(uint32 expected, uint32 actual);
    error WrongChain(uint256 expected, uint64 actual);
    error WrongExchange(address expected, address actual);
    error WrongConfig(bytes32 expected, bytes32 actual);
    error WrongBatchId(uint64 expected, uint64 actual);
    error StaleStateRoot(bytes32 expected, bytes32 actual);
    error WrongDepositCursor(uint64 expected, uint64 actual);
    error InvalidDepositCursorAdvance(uint64 oldCursor, uint64 newCursor, uint64 nextDepositId);
    error ConsumedDepositsHashMismatch(bytes32 expected, bytes32 actual);
    error WithdrawalsHashMismatch(bytes32 expected, bytes32 actual);
    error NativeWithdrawalFailed(address recipient, uint256 amount);
    error ZeroVerifier();
    error ZeroDepositAmount();
    error WrongForcedWithdrawalCursor(uint64 expected, uint64 actual);
    error InvalidForcedWithdrawalCursorAdvance(uint64 oldCursor, uint64 newCursor, uint64 nextForcedWithdrawalId);
    error ConsumedForcedWithdrawalsHashMismatch(bytes32 expected, bytes32 actual);
    error ForcedWithdrawalsHashMismatch(bytes32 expected, bytes32 actual);
    error NativeForcedWithdrawalFailed(address account, uint256 amount);
    error ZeroForcedWithdrawalAmount();
    error EscapeModeActive();
    error EscapeModeNotActive();
    error NoPendingForcedWithdrawal();
    error ForcedWithdrawalDeadlineNotElapsed(uint64 deadline);
    error InvalidEscapeProof(bytes32 expected, bytes32 actual);
    error AlreadyEscaped(address account);
    error NativeEscapeWithdrawalFailed(address account, uint256 amount);

    event DepositQueued(uint64 indexed depositId, address indexed account, address indexed asset, uint128 amount);

    event WithdrawalExecuted(
        address indexed account, address indexed recipient, address indexed asset, uint128 amount, uint64 nonce
    );

    event ForcedWithdrawalRequested(
        uint64 indexed id, address indexed account, address indexed asset, uint128 amount, uint64 deadline
    );

    event ForcedWithdrawalExecuted(
        address indexed account, address indexed asset, uint128 amount, uint64 indexed id
    );

    event EscapeModeActivated(uint64 requestId, uint64 deadline);

    event EscapeWithdrawn(address indexed account, address indexed asset, uint128 amount);

    event BatchSettled(
        uint64 indexed batchId,
        bytes32 indexed oldStateRoot,
        bytes32 indexed newStateRoot,
        bytes32 batchHash,
        bytes32 tradesHash
    );

    function settle(
        bytes calldata publicValues,
        bytes calldata proof,
        Withdrawal[] calldata withdrawals,
        ForcedWithdrawal[] calldata forcedWithdrawals
    ) external;

    /// Locks native ETH and appends it to the deposit queue. `asset` is address(0).
    function deposit() external payable returns (uint64 depositId);

    /// Locks an exact amount of ERC-20 tokens and appends it to the deposit queue.
    function deposit(address token, uint256 amount) external payable returns (uint64 depositId);

    /// Registers a request to withdraw up to `amount` of `asset` from the caller's internal
    /// balance, bypassing the operator. The operator drains at most `amount`; if it isn't
    /// processed before the deadline, anyone may activate escape mode.
    function requestForcedWithdrawal(address asset, uint128 amount) external returns (uint64 requestId);

    /// Freezes ordinary settlement once the oldest pending forced withdrawal request's
    /// deadline has elapsed. Permissionless.
    function activateEscapeMode() external;

    /// Drains every balance in `balances` to `id`, once escape mode is active and given a
    /// valid Merkle proof that `id`'s account (with `nextNonce` and exactly `balances`) is a
    /// leaf of the frozen `stateRoot`. Permissionless — funds only ever move to `id` itself,
    /// so anyone may submit the (fully public) proof on the account's behalf. Each account may
    /// only do this once.
    function escapeWithdraw(
        address id,
        uint64 nextNonce,
        PatriciaProof.AssetBalance[] calldata balances,
        PatriciaProof.SideNode[] calldata sideNodes
    ) external;

    function escaped(address account) external view returns (bool);

    function deposits(uint64 depositId) external view returns (address account, address asset, uint128 amount);

    function forcedWithdrawalRequests(uint64 requestId)
        external
        view
        returns (address account, address asset, uint128 amount, uint64 deadline);

    function nextDepositId() external view returns (uint64);

    function nextUnprocessedDeposit() external view returns (uint64);

    function nextForcedWithdrawalId() external view returns (uint64);

    function nextUnprocessedForcedWithdrawal() external view returns (uint64);

    function escapeMode() external view returns (bool);

    function FORCED_WITHDRAWAL_DELAY() external view returns (uint64);

    function stateRoot() external view returns (bytes32);

    function nextBatchId() external view returns (uint64);
}
