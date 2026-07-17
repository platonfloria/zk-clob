// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

interface IZkClob {
    struct BatchMetadata {
        uint32 protocolVersion;
        uint64 chainId;
        bytes32 exchangeId;
        uint64 batchId;
    }

    struct PublicOutput {
        BatchMetadata metadata;
        bytes32 oldStateRoot;
        bytes32 newStateRoot;
        bytes32 configHash;
        bytes32 batchHash;
        bytes32 tradesHash;
    }

    error InvalidPublicValuesLength(uint256 actual);
    error WrongProtocolVersion(uint32 expected, uint32 actual);
    error WrongChain(uint256 expected, uint64 actual);
    error WrongExchange(bytes32 expected, bytes32 actual);
    error WrongConfig(bytes32 expected, bytes32 actual);
    error WrongBatchId(uint64 expected, uint64 actual);
    error StaleStateRoot(bytes32 expected, bytes32 actual);
    error ZeroVerifier();

    event BatchSettled(
        uint64 indexed batchId,
        bytes32 indexed oldStateRoot,
        bytes32 indexed newStateRoot,
        bytes32 batchHash,
        bytes32 tradesHash
    );

    function settle(bytes calldata publicValues, bytes calldata proof) external;

    function stateRoot() external view returns (bytes32);

    function nextBatchId() external view returns (uint64);
}
