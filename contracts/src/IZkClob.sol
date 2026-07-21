// SPDX-License-Identifier: MIT
pragma solidity 0.8.35;

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
        bytes32 asset;
        uint128 amount;
        uint64 nonce;
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
        bytes32 withdrawalsHash;
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
    error InvalidWithdrawalAsset(bytes32 asset);
    error NativeWithdrawalFailed(address recipient, uint256 amount);
    error ZeroVerifier();
    error ZeroDepositAmount();

    event DepositQueued(uint64 indexed depositId, address indexed account, address indexed asset, uint128 amount);

    event WithdrawalExecuted(
        address indexed account, address indexed recipient, bytes32 indexed asset, uint128 amount, uint64 nonce
    );

    event BatchSettled(
        uint64 indexed batchId,
        bytes32 indexed oldStateRoot,
        bytes32 indexed newStateRoot,
        bytes32 batchHash,
        bytes32 tradesHash
    );

    function settle(bytes calldata publicValues, bytes calldata proof, Withdrawal[] calldata withdrawals) external;

    /// Locks native ETH and appends it to the deposit queue. `asset` is address(0).
    function deposit() external payable returns (uint64 depositId);

    /// Locks an exact amount of ERC-20 tokens and appends it to the deposit queue.
    function deposit(address token, uint256 amount) external payable returns (uint64 depositId);

    function deposits(uint64 depositId) external view returns (address account, address asset, uint128 amount);

    function nextDepositId() external view returns (uint64);

    function nextUnprocessedDeposit() external view returns (uint64);

    function stateRoot() external view returns (bytes32);

    function nextBatchId() external view returns (uint64);
}
