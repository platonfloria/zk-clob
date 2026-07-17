// SPDX-License-Identifier: MIT
pragma solidity 0.8.20;

import {ISP1Verifier} from "@sp1-contracts/ISP1Verifier.sol";

import {IZkClob} from "./IZkClob.sol";

contract ZkClob is IZkClob {
    uint256 private constant PUBLIC_VALUES_LENGTH = 9 * 32;

    ISP1Verifier public immutable VERIFIER;
    bytes32 public immutable PROGRAM_VKEY;
    bytes32 public immutable EXCHANGE_ID;
    bytes32 public immutable CONFIG_HASH;
    uint32 public immutable PROTOCOL_VERSION;

    bytes32 public override stateRoot;
    uint64 public override nextBatchId;

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

    function settle(bytes calldata publicValues, bytes calldata proof) external {
        if (publicValues.length != PUBLIC_VALUES_LENGTH) {
            revert InvalidPublicValuesLength(publicValues.length);
        }

        PublicOutput memory output = abi.decode(publicValues, (PublicOutput));
        BatchMetadata memory metadata = output.metadata;

        if (metadata.protocolVersion != PROTOCOL_VERSION) {
            revert WrongProtocolVersion(PROTOCOL_VERSION, metadata.protocolVersion);
        }
        if (uint256(metadata.chainId) != block.chainid) {
            revert WrongChain(block.chainid, metadata.chainId);
        }
        if (metadata.exchangeId != EXCHANGE_ID) {
            revert WrongExchange(EXCHANGE_ID, metadata.exchangeId);
        }
        if (output.configHash != CONFIG_HASH) {
            revert WrongConfig(CONFIG_HASH, output.configHash);
        }
        if (metadata.batchId != nextBatchId) {
            revert WrongBatchId(nextBatchId, metadata.batchId);
        }
        if (output.oldStateRoot != stateRoot) {
            revert StaleStateRoot(stateRoot, output.oldStateRoot);
        }

        VERIFIER.verifyProof(PROGRAM_VKEY, publicValues, proof);

        stateRoot = output.newStateRoot;
        nextBatchId++;

        emit BatchSettled(
            metadata.batchId, output.oldStateRoot, output.newStateRoot, output.batchHash, output.tradesHash
        );
    }
}
